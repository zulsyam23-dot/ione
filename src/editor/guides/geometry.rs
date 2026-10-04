//! Galley pixel geometry: char rects/lines from the galley, plus the low-level
//! guide strokes. Everything is measured through `pos_from_cursor` so every
//! guide lands on the same pixel column as the glyph it marks.

use eframe::egui::{self, Color32, Pos2, Rect};
use egui::epaint::text::cursor::LayoutCursor;
use egui::text::{CCursor, CharIndex};

use crate::editor::guides::Pair;

/// Char index at which every galley row starts.
///
/// egui's own `layout_from_cursor` walks rows from the top until it reaches the
/// cursor, so one lookup is O(rows). Asking it about every bracket pair in a
/// file is therefore O(pairs x rows): on a 4 MB file that single frame cost
/// ~19 s of frozen UI. This index is built once per galley (one linear pass)
/// and answers the same mapping by binary search.
#[derive(Clone, Default)]
pub struct RowStarts(std::sync::Arc<Vec<usize>>);

impl RowStarts {
    pub fn new(galley: &egui::Galley) -> Self {
        let mut starts = Vec::with_capacity(galley.rows.len());
        let mut index = 0usize;
        for row in &galley.rows {
            starts.push(index);
            index += row.char_count_including_newline().0;
        }
        Self(std::sync::Arc::new(starts))
    }

    /// The same mapping as `galley.layout_from_cursor(cursor)` with
    /// `prefer_next_row: false`, in O(log rows).
    pub fn layout(&self, galley: &egui::Galley, idx: usize) -> LayoutCursor {
        if self.0.is_empty() {
            return galley.layout_from_cursor(cursor_at(idx));
        }
        let row = self
            .0
            .partition_point(|&start| start <= idx)
            .saturating_sub(1)
            .min(self.0.len() - 1);
        LayoutCursor {
            row,
            column: CharIndex(idx - self.0[row]),
        }
    }
}

fn cursor_at(idx: usize) -> CCursor {
    CCursor {
        index: CharIndex(idx),
        prefer_next_row: false,
    }
}

/// Screen rectangle of the char at `idx` (0-width column + full row height).
/// Every caller derives `idx` from a `FoldView::d2r`/scan index, which is
/// always `< display_char_count`, so the clamp was pure waste; only `multi`
/// passes `de + 1` == end (valid for `pos_from_cursor`).
pub fn char_rect(galley: &egui::Galley, origin: Pos2, idx: usize) -> Rect {
    let r = galley.pos_from_cursor(cursor_at(idx));
    r.translate(origin.to_vec2())
}

/// `char_rect` through a prebuilt row index: same result, O(log rows) instead
/// of O(rows). Use this wherever the index is at hand (whole-file scans).
pub fn char_rect_indexed(
    galley: &egui::Galley,
    origin: Pos2,
    rows: &RowStarts,
    idx: usize,
) -> Rect {
    galley
        .pos_from_layout_cursor(&rows.layout(galley, idx))
        .translate(origin.to_vec2())
}

pub fn char_line(galley: &egui::Galley, idx: usize) -> usize {
    galley.layout_from_cursor(cursor_at(idx)).row
}

/// `char_line` through a prebuilt row index (O(log rows)).
pub fn char_line_indexed(galley: &egui::Galley, rows: &RowStarts, idx: usize) -> usize {
    rows.layout(galley, idx).row
}

/// Column (rounded screen x) of a pair's closing bracket guide, if it spans
/// more than one line. Indexed: whole-file pair scans must never go through
/// egui's linear cursor lookup.
pub fn pair_guide_x_indexed(
    galley: &egui::Galley,
    origin: Pos2,
    rows: &RowStarts,
    p: Pair,
) -> Option<f32> {
    if p.close == usize::MAX
        || char_line_indexed(galley, rows, p.open) == char_line_indexed(galley, rows, p.close)
    {
        return None;
    }
    Some(
        char_rect_indexed(galley, origin, rows, p.close)
            .left()
            .round(),
    )
}

/// A square-cap vertical guide: a thin rect reads as a crisp uniform stroke
/// of exactly `width` px regardless of how long the block is. (Rounded caps
/// the size of the half-width turn short guides into pills that look thinner.)
pub fn guide_line(
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui::text::{LayoutJob, TextFormat};

    /// The index must agree with egui's own linear lookup on every char,
    /// including the newline positions and the very end of the buffer - the
    /// guides depend on that agreement to land on the right glyph.
    #[test]
    fn row_starts_agree_with_egui_row_by_row() {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        let text = "fn a() {\n    b();\n}\nlet c = 1;\n";
        let mut job = LayoutJob::default();
        job.append(
            text,
            0.0,
            TextFormat::simple(egui::FontId::monospace(12.0), Color32::WHITE),
        );
        let galley = {
            let mut job = Some(job);
            let mut galley = None;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    galley = Some(
                        ui.fonts_mut(|fonts| fonts.layout_job(job.take().expect("shaped once"))),
                    );
                });
            });
            // This test never reaches a real paint pass, so hand the atlas
            // deltas back instead of dropping them (egui panics on that).
            let mut output = output;
            output.textures_delta.clear();
            galley.expect("a galley comes back from the closure")
        };
        let rows = RowStarts::new(&galley);
        let chars = text.chars().count();
        for idx in 0..=chars {
            assert_eq!(
                rows.layout(&galley, idx),
                galley.layout_from_cursor(cursor_at(idx)),
                "row lookup disagrees at char {idx}"
            );
            assert_eq!(
                char_rect_indexed(&galley, Pos2::ZERO, &rows, idx),
                char_rect(&galley, Pos2::ZERO, idx),
                "char rect disagrees at char {idx}"
            );
        }
    }
}
