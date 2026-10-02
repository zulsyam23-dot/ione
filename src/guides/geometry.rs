//! Galley pixel geometry: char rects/lines from the galley, plus the low-level
//! guide strokes. Everything is measured through `pos_from_cursor` so every
//! guide lands on the same pixel column as the glyph it marks.

use eframe::egui::{self, Color32, Pos2, Rect};
use egui::text::{CCursor, CharIndex};

use crate::guides::Pair;

/// Screen rectangle of the char at `idx` (0-width column + full row height).
/// Every caller derives `idx` from a `FoldView::d2r`/scan index, which is
/// always `< display_char_count`, so the clamp was pure waste; only `multi`
/// passes `de + 1` == end (valid for `pos_from_cursor`).
pub(crate) fn char_rect(galley: &egui::Galley, origin: Pos2, idx: usize) -> Rect {
    let r = galley.pos_from_cursor(CCursor {
        index: CharIndex(idx),
        prefer_next_row: false,
    });
    r.translate(origin.to_vec2())
}

pub(crate) fn char_line(galley: &egui::Galley, idx: usize) -> usize {
    galley
        .layout_from_cursor(CCursor {
            index: CharIndex(idx),
            prefer_next_row: false,
        })
        .row
}

/// Column (rounded screen x) of a pair's closing bracket guide, if it spans
/// more than one line.
pub(crate) fn pair_guide_x(galley: &egui::Galley, origin: Pos2, p: Pair) -> Option<f32> {
    if p.close == usize::MAX || char_line(galley, p.open) == char_line(galley, p.close) {
        return None;
    }
    Some(char_rect(galley, origin, p.close).left().round())
}

/// A square-cap vertical guide: a thin rect reads as a crisp uniform stroke
/// of exactly `width` px regardless of how long the block is. (Rounded caps
/// the size of the half-width turn short guides into pills that look thinner.)
pub(crate) fn guide_line(
    painter: &egui::Painter,
    x: f32,
    y0: f32,
    y1: f32,
    width: f32,
    color: Color32,
) {
    let half = width / 2.0;
    let rect = Rect::from_min_max(Pos2::new(x - half, y0), Pos2::new(x + half, y1));
    painter.rect_filled(rect, egui::CornerRadius::ZERO, color);
}
