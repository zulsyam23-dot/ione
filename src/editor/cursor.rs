//! Pure text-position math (no egui): line/column derived from a cursor's char
//! offset.

/// 1-based line / 0-based column of a text prefix ending at the cursor.
pub(crate) fn line_col_of(text: &str) -> Option<(usize, usize)> {
    match text.rfind('\n') {
        Some(last_newline) => {
            let line = text[..last_newline].matches('\n').count() + 1;
            let col = text[last_newline + 1..].chars().count();
            Some((line, col))
        }
        None => Some((0, text.chars().count())),
    }
}