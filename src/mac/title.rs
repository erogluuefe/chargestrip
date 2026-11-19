//! The menu bar title, drawn as one monochrome *template* image: an SF Symbols glyph and the
//! percentage (system menu bar font) for each device. macOS tints template images like every other
//! menu bar item: white on a dark menu bar, black on a light one.
//!
//! State is shown without color (templates only keep alpha): a bolt after a charging device, a
//! warning sign at 15 % or less, and a dimmed item when the device is off or asleep.

use crate::model::{Kind, Shown, Tone};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::AllocAnyThread;
use objc2_app_kit::{
    NSAttributedStringNSStringDrawing, NSColor, NSCompositingOperation, NSFont, NSFontAttributeName,
    NSFontWeightRegular, NSForegroundColorAttributeName, NSImage, NSImageSymbolConfiguration,
};
use objc2_foundation::{NSAttributedString, NSDictionary, NSPoint, NSRect, NSSize, NSString};

/// Height of the image; the menu bar is 22–24 pt tall.
const HEIGHT: f64 = 18.0;
/// Point size of the device symbols and of the small state marks.
const SYMBOL_PT: f64 = 13.0;
const MARK_PT: f64 = 9.0;
/// Gaps between symbol and text, text and mark, and between devices.
const GAP: f64 = 3.0;
const ITEM_GAP: f64 = 9.0;
/// Opacity of a device that is off or asleep.
const DIMMED: f64 = 0.4;

/// SF Symbols name for each device kind.
fn symbol(kind: Kind) -> &'static str {
    match kind {
        Kind::Headset => "headphones",
        Kind::Mouse => "computermouse",
        Kind::Keyboard => "keyboard",
        Kind::Gamepad => "gamecontroller",
        Kind::Other => "antenna.radiowaves.left.and.right",
    }
}

/// Small mark after the percentage.
fn mark(tone: Tone) -> Option<&'static str> {
    match tone {
        Tone::Charging => Some("bolt.fill"),
        Tone::Low => Some("exclamationmark.triangle.fill"),
        Tone::Normal | Tone::Inactive => None,
    }
}

/// One piece of the title: a symbol or a text.
enum Piece {
    Symbol(Retained<NSImage>),
    Text(Retained<NSAttributedString>),
}

fn size_of(p: &Piece) -> NSSize {
    match p {
        Piece::Symbol(img) => img.size(),
        Piece::Text(s) => s.size(),
    }
}

/// Renders the title image; `None` if a symbol is missing (e.g. older macOS), in which case the
/// caller falls back to a plain text title.
pub fn render(items: &[Shown]) -> Option<Retained<NSImage>> {
    let font = NSFont::menuBarFontOfSize(0.0);
    let mut pieces: Vec<(Piece, f64, f64)> = Vec::new(); // (piece, gap before, opacity)
    if items.is_empty() {
        pieces.push((Piece::Symbol(sf("battery.0", SYMBOL_PT)?), 0.0, DIMMED));
        pieces.push((Piece::Text(text("–", &font, DIMMED)), GAP, DIMMED));
    }
    for (i, item) in items.iter().enumerate() {
        let alpha = if item.tone == Tone::Inactive { DIMMED } else { 1.0 };
        let lead = if i == 0 { 0.0 } else { ITEM_GAP };
        pieces.push((Piece::Symbol(sf(symbol(item.kind), SYMBOL_PT)?), lead, alpha));
        pieces.push((Piece::Text(text(&item.text, &font, alpha)), GAP, alpha));
        if let Some(name) = mark(item.tone) {
            pieces.push((Piece::Symbol(sf(name, MARK_PT)?), 1.0, alpha));
        }
    }

    let width: f64 = pieces.iter().map(|(p, gap, _)| gap + size_of(p).width).sum::<f64>().ceil();
    // Block-based drawing: macOS calls it again for every backing scale (Retina, external display),
    // so the result stays sharp everywhere.
    let draw = RcBlock::new(move |_: NSRect| -> Bool {
        let mut x = 0.0;
        for (piece, gap, alpha) in &pieces {
            x += gap;
            let size = size_of(piece);
            let y = ((HEIGHT - size.height) / 2.0).round();
            match piece {
                Piece::Symbol(img) => img.drawInRect_fromRect_operation_fraction(
                    NSRect::new(NSPoint::new(x, y), size),
                    NSRect::ZERO,
                    NSCompositingOperation::SourceOver,
                    *alpha,
                ),
                Piece::Text(s) => s.drawAtPoint(NSPoint::new(x, y)),
            }
            x += size.width;
        }
        Bool::YES
    });
    let image = NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(width.max(1.0), HEIGHT), false, &draw);
    image.setTemplate(true);
    Some(image)
}

/// An SF Symbols image at the given point size.
fn sf(name: &str, points: f64) -> Option<Retained<NSImage>> {
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None)?;
    let config = NSImageSymbolConfiguration::configurationWithPointSize_weight(points, unsafe { NSFontWeightRegular });
    image.imageWithSymbolConfiguration(&config)
}

/// Text in the menu bar font; only its alpha matters in a template image.
fn text(s: &str, font: &NSFont, alpha: f64) -> Retained<NSAttributedString> {
    let color = NSColor::colorWithWhite_alpha(0.0, alpha);
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let values: [&AnyObject; 2] = [font.as_ref(), color.as_ref()];
    let attributes = NSDictionary::from_slices(&keys, &values);
    unsafe { NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &NSString::from_str(s), Some(&attributes)) }
}
