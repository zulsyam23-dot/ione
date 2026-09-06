use eframe::egui::{self, Color32};

use crate::style::{apply_style, Palette};
use crate::theme::Theme;

pub(super) const HANDLE: f32 = 6.0;

pub(super) fn load_logo(ctx: &egui::Context) -> egui::TextureHandle {
    let bytes = include_bytes!("..\\..\\assets\\icons\\app\\1770523897143.ico");
    let img = image::load_from_memory(bytes)
        .expect("Failed to load logo")
        .into_rgba8();
    let (w, h) = img.dimensions();
    let color = egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], img.as_raw());
    ctx.load_texture("ione_logo", color, egui::TextureOptions::LINEAR)
}

pub(super) fn drag_vertical_splitter(ui: &mut egui::Ui, color: Color32, frac: &mut f32, total_h: f32) {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), HANDLE),
        egui::Sense::drag(),
    );
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    ui.painter().rect(
        rect,
        0.0,
        if resp.hovered() || resp.dragged() {
            color
        } else {
            Color32::TRANSPARENT
        },
        egui::Stroke::NONE,
        egui::StrokeKind::Inside,
    );
    let dy = resp.drag_delta().y;
    if dy != 0.0 && total_h > 0.0 {
        *frac = (*frac - dy / total_h).clamp(0.1, 0.9);
    }
}

pub(super) fn apply_egui_theme(ctx: &egui::Context, theme: Theme) {
    apply_style(
        ctx,
        if theme.is_dark() {
            Palette::dark()
        } else {
            Palette::light()
        },
    );
}

/// Byte ranges of every match of `query` in `content`, always in the ORIGINAL
/// content's byte coordinates. The case-insensitive path walks char-by-char and
/// folds via `to_lowercase` with a back-map, so chars whose fold changes length
/// (e.g. İ → i̇) can't produce indices that slice `content` off-boundary.
fn match_ranges(content: &str, query: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    if case_sensitive {
        return content
            .match_indices(query)
            .map(|(i, m)| (i, i + m.len()))
            .collect();
    }
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    let mut folded = String::with_capacity(content.len());
    let mut map = Vec::new(); // folded char idx -> original char idx (into `chars`)
    for (k, (_, c)) in chars.iter().enumerate() {
        for fc in c.to_lowercase() {
            folded.push(fc);
            map.push(k);
        }
    }
    let query_lower = query.to_lowercase();
    let mut out = Vec::new();
    for (s, m) in folded.match_indices(&query_lower) {
        // `s` is a byte offset into `folded`, but `map` is indexed by CHAR
        // count, so convert (s can sit mid-char for multi-byte folds).
        let start_ci = folded[..s].chars().count();
        let end_ci = start_ci + m.chars().count();
        let start_k = map[start_ci];
        let (b, c) = chars[map[end_ci - 1]];
        out.push((chars[start_k].0, b + c.len_utf8()));
    }
    out
}

pub(super) fn replace_first(content: &mut String, query: &str, replacement: &str, case_sensitive: bool) {
    if let Some((s, e)) = match_ranges(content, query, case_sensitive).into_iter().next() {
        content.replace_range(s..e, replacement);
    }
}

pub(super) fn replace_all_in_content(
    content: &mut String,
    query: &str,
    replacement: &str,
    case_sensitive: bool,
) {
    let ranges = match_ranges(content, query, case_sensitive);
    if ranges.is_empty() {
        return;
    }
    let mut result = String::with_capacity(content.len());
    let mut last = 0;
    for (s, e) in ranges {
        result.push_str(&content[last..s]);
        result.push_str(replacement);
        last = e;
    }
    result.push_str(&content[last..]);
    *content = result;
}

fn char_to_byte(content: &str, ci: usize) -> usize {
    content.chars().take(ci).map(char::len_utf8).sum()
}

/// Next match at/after char `start_char` (wraps to the first), as a
/// char-index range in the real buffer.
pub(super) fn find_next_range(
    content: &str,
    query: &str,
    case_sensitive: bool,
    start_char: usize,
) -> Option<(usize, usize)> {
    let byte_start = char_to_byte(content, start_char);
    let ranges = match_ranges(content, query, case_sensitive);
    ranges
        .iter()
        .find(|&&(s, _)| s >= byte_start)
        .or_else(|| ranges.first())
        .map(|&(s, e)| (content[..s].chars().count(), content[..e].chars().count()))
}

/// Previous match strictly before char `start_char` (wraps to the last), as a
/// char-index range in the real buffer.
pub(super) fn find_prev_range(
    content: &str,
    query: &str,
    case_sensitive: bool,
    start_char: usize,
) -> Option<(usize, usize)> {
    let byte_start = char_to_byte(content, start_char);
    let ranges = match_ranges(content, query, case_sensitive);
    ranges
        .iter()
        .rev()
        .find(|&&(_, e)| e < byte_start)
        .or_else(|| ranges.last())
        .map(|&(s, e)| (content[..s].chars().count(), content[..e].chars().count()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_insensitive_replace_survives_length_changing_fold() {
        // 'i' matches both İ (whose lowercase fold is 2 chars) and plain i.
        let mut content = "İstanbul İzmir".to_string();
        replace_all_in_content(&mut content, "i", "X", false);
        assert_eq!(content, "Xstanbul XzmXr");
        // The multi-char fold match (İ itself) replaces whole chars, not bytes.
        let mut one = "İstanbul".to_string();
        replace_first(&mut one, "İ", "x", false);
        assert_eq!(one, "xstanbul");
    }
}