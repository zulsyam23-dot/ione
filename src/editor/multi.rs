//! Multi-select (limited): Ctrl+D selects occurrences of one word; a single
//! keystroke then edits all of them at once. State is a [`MultiSel`] stored on
//! the tab — `word` is kept only so Ctrl+D can keep adding the next occurrence;
//! `ranges` are the live caret/selection positions, updated after each batch
//! edit. Only the "one direction" edits are batched: insert, backspace, delete,
//! enter, tab — no multi clipboard/undo/cursor movement (needs a full custom
//! text widget; declined scope).

use eframe::egui::{self, Event};

/// Active multi-select on a tab.
#[derive(Clone, Debug)]
pub struct MultiSel {
    /// Seed word Ctrl+D is extending.
    pub word: String,
    /// Selection ranges (real-buffer byte coords), sorted, non-overlapping.
    pub ranges: Vec<(usize, usize)>,
}

/// Every occurrence of `word` in `content` as byte-index ranges.
pub fn occurrences(content: &str, word: &str) -> Vec<(usize, usize)> {
    if word.is_empty() {
        return Vec::new();
    }
    content
        .match_indices(word)
        .map(|(i, m)| (i, i + m.len()))
        .collect()
}

/// The next occurrence not yet selected, preferring one at/after `after` and
/// wrapping to the first — for Ctrl+D "add another".
pub fn next_occurrence(
    content: &str,
    word: &str,
    after: usize,
    selected: &[(usize, usize)],
) -> Option<(usize, usize)> {
    let is_selected = |&(s, _): &(usize, usize)| selected.iter().any(|&(ss, ee)| ss <= s && s < ee);
    let candidates: Vec<_> = occurrences(content, word)
        .into_iter()
        .filter(|r| !is_selected(r))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates
        .iter()
        .copied()
        .find(|&(s, _)| s >= after)
        .or_else(|| candidates.first().copied())
}

/// A batch edit applied at every selected range simultaneously.
#[derive(Clone, Debug)]
pub enum Edit {
    Insert(String),
    Backspace,
    Delete,
    Enter,
    InsertTab,
}

/// Apply `edit` at every `range` and return the new caret positions (one per
/// range, zero-width) so subsequent keystrokes keep appending. Ranges are
/// resolved against the ORIGINAL buffer, so the math is exact regardless of
/// how earlier ranges shift the text. `None` when nothing was edited.
pub fn apply_edit(content: &mut String, ranges: &[(usize, usize)], edit: &Edit) -> Option<Vec<(usize, usize)>> {
    if ranges.is_empty() {
        return None;
    }
    let out: String = content.clone();
    // Resolve every range against the original buffer first.
    let mut ds: Vec<(usize, usize, String)> = Vec::new(); // (start,end,insert)
    for &(rs, re) in ranges {
        let (s, e) = (rs.min(re), rs.max(re));
        if let Some(d) = delta_for(&out, s, e, edit) {
            ds.push(d);
        }
    }
    if ds.is_empty() {
        return None;
    }
    // Build the result by walking ranges in order, splicing around them.
    let mut result = String::with_capacity(out.len());
    let mut last = 0usize;
    let mut carets = Vec::with_capacity(ds.len());
    for (start, end, insert) in ds {
        if start < last {
            continue;
        }
        result.push_str(&out[last..start]);
        result.push_str(&insert);
        last = end;
        // Caret lands at the end of this range's insert in the EDITED buffer.
        carets.push((result.len(), result.len()));
    }
    if last < out.len() {
        result.push_str(&out[last..]);
    }
    *content = result;
    Some(carets)
}

/// Per-range effect as `(remove_start, remove_end, insert)` on the ORIGINAL
/// buffer, or `None` when the range can't do the edit (Delete at end of
/// buffer, Backspace at start).
fn delta_for(content: &str, s: usize, e: usize, edit: &Edit) -> Option<(usize, usize, String)> {
    let selection = s < e;
    match edit {
        Edit::Insert(text) => Some((s, if selection { e } else { s }, text.clone())),
        Edit::InsertTab => Some((s, if selection { e } else { s }, "\t".to_string())),
        Edit::Enter => Some((s, if selection { e } else { s }, "\n".to_string())),
        Edit::Backspace => {
            if selection {
                return Some((s, e, String::new()));
            }
            let prefix = &content[..s];
            let before = prefix.chars().next_back()?;
            let len = before.len_utf8();
            Some((s - len, s, String::new()))
        }
        Edit::Delete => {
            if selection {
                return Some((s, e, String::new()));
            }
            let next = content[s..].chars().next()?;
            Some((s, s + next.len_utf8(), String::new()))
        }
    }
}

/// Paint the multi-select highlight: a translucent rect over each range.
/// `view` maps real buffer coords to display coords; ranges hidden behind a
/// fold are skipped. `char_rect` measures from the galley.
pub fn draw_selection(
    ui: &egui::Ui,
    galley: &egui::Galley,
    origin: egui::Pos2,
    view: &crate::editor::folds::FoldView,
    ranges: &[(usize, usize)],
    color: egui::Color32,
) {
    if ranges.is_empty() {
        return;
    }
    // d2r is display->real; linear scan is fine at sub-100 range counts.
    let to_display = |real: usize| -> Option<usize> { view.d2r.iter().position(|&r| r == real) };
    for &(rs, re) in ranges {
        let Some(ds) = to_display(rs) else { continue };
        let Some(de) = to_display(re.saturating_sub(1)) else { continue };
        let start = crate::guides::geometry::char_rect(galley, origin, ds);
        let end = crate::guides::geometry::char_rect(galley, origin, de + 1);
        let top = start.top().min(end.top());
        let bottom = start.bottom().max(end.bottom());
        let rect = egui::Rect::from_min_max(
            egui::pos2(start.left(), top),
            egui::pos2(end.left(), bottom),
        );
        if rect.width() > 0.0 && rect.height() > 0.0 {
            ui.painter().rect_filled(rect, 0.0, color);
        }
    }
}

/// The word starting at or spanning char `pos` in `content` (byte positions),
/// or `None`. Words are `[A-Za-z0-9_]` runs — the common identifier shape.
pub fn word_at(content: &str, byte_pos: usize) -> Option<String> {
    let chars: Vec<(usize, char)> = content.char_indices().collect();
    let idx = chars.partition_point(|&(b, _)| b < byte_pos);
    let is_word = |c: char| c == '_' || c.is_alphanumeric();

    // Grow left while on word chars.
    let mut start = idx.min(chars.len());
    while start > 0 && is_word(chars[start - 1].1) {
        start -= 1;
    }
    // Grow right to the end of the word run.
    let mut end = start;
    while end < chars.len() && is_word(chars[end].1) {
        end += 1;
    }
    if end <= start {
        return None;
    }
    let s = chars[start].0;
    let last_b = chars[end - 1].0;
    let e = last_b + chars[end - 1].1.len_utf8();
    Some(content[s..e].to_string())
}

/// A single frame's worth of "is this the one edit to batch?" reading.
pub fn is_batchable(event: &Event) -> bool {
    matches!(
        event,
        Event::Text(_)
            | Event::Key {
                key: egui::Key::Backspace | egui::Key::Delete | egui::Key::Enter | egui::Key::Tab,
                pressed: true,
                ..
            }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_replaces_selected_words_and_returns_carets() {
        let mut c = "a foo b foo c foo".to_string();
        let foo = occurrences(&c, "foo");
        let carets = apply_edit(&mut c, &foo, &Edit::Insert("X".into())).unwrap();
        assert_eq!(c, "a X b X c X");
        // Carets sit right after each X in the edited buffer.
        assert_eq!(carets, vec![(3, 3), (7, 7), (11, 11)]);
        // Typing 'y' now appends at every caret.
        apply_edit(&mut c, &carets, &Edit::Insert("y".into()));
        assert_eq!(c, "a Xy b Xy c Xy");
    }

    #[test]
    fn backspace_at_carets_removes_char_before_each() {
        let mut c = "az bz cz".to_string();
        let ranges: Vec<(usize, usize)> = c.match_indices('z').map(|(i, _)| (i, i)).collect();
        apply_edit(&mut c, &ranges, &Edit::Backspace);
        assert_eq!(c, "z z z");
    }

    #[test]
    fn backspace_at_start_skips_that_range() {
        let mut c = "z az".to_string();
        let ranges: Vec<(usize, usize)> = c.match_indices('z').map(|(i, _)| (i, i)).collect();
        apply_edit(&mut c, &ranges, &Edit::Backspace);
        // Range 0 (start) can't backspace; the 'z' after 'a' removes 'a'.
        assert_eq!(c, "z z");
    }

    #[test]
    fn delete_removes_char_after_each_caret() {
        let mut c = "za zb zc".to_string();
        let ranges: Vec<(usize, usize)> = c.match_indices('z').map(|(i, _)| (i, i)).collect();
        apply_edit(&mut c, &ranges, &Edit::Delete);
        assert_eq!(c, "a b c");
    }

    #[test]
    fn word_at_finds_identifier_spanning_cursor() {
        assert_eq!(word_at("let count = 5", 4), Some("count".to_string()));
        assert_eq!(word_at("x_1", 2), Some("x_1".to_string()));
        assert_eq!(word_at("  .", 2), None);
    }

    #[test]
    fn next_occurrence_wraps_and_skips_selected() {
        let c = "foo foo foo";
        let sel = vec![(0, 3)];
        assert_eq!(next_occurrence(c, "foo", 3, &sel), Some((4, 7)));
        assert_eq!(next_occurrence(c, "foo", 7, &sel), Some((8, 11)));
        let all: Vec<_> = occurrences(c, "foo");
        assert_eq!(next_occurrence(c, "foo", 11, &all), None);
    }
}