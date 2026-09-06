//! Code folding: a [`TextBuffer`] whose displayed text hides folded regions,
//! plus the chevron toggles used in the gutter to expand/collapse them.
//!
//! `TextEdit` edits whatever `&dyn TextBuffer` it is given, so folding is a
//! *presentation* concern: the folded text (open line kept, interior hidden
//! behind a `⋯` marker) is what gets painted, while all edits land in the real
//! buffer — saving always writes the untouched file. Any edit touching a
//! marker auto-unfolds that region first (the standard "click the marker and
//! type to expand" behavior), so the mapping can never corrupt content.

use std::any::TypeId;
use std::ops::Range;

use eframe::egui;
use egui::text::CharIndex;
use egui::TextBuffer;

use crate::guides::brackets::BracePair;
use crate::icons::{Icon, Icons};
use crate::tabs::Fold;

const MARKER: &str = "⋯";

// ---------------------------------------------------------------------------
// FoldView: display text + mapping, computed from (content, folds) with no
// mutable borrows — safe to call from the editor before/after the widget.
// ---------------------------------------------------------------------------

/// One displayed row.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Row {
    /// Real 0-based line number this row shows.
    pub(crate) line: usize,
    /// Char index where this row starts in the display text.
    pub(crate) char_start: usize,
    /// Display char index of the inline `⋯` marker at the end of this row.
    pub(crate) marker_char: Option<usize>,
    /// Open-brace char index of the fold this row's `⋯` marker stands for.
    pub(crate) marker: Option<usize>,
}

/// Immutable description of how `content` looks with `folds` applied.
#[derive(Clone, Debug)]
pub(crate) struct FoldView {
    pub(crate) display: String,
    /// display char index -> real char index (len = display chars + 1).
    pub(crate) d2r: Vec<usize>,
    /// Rows in display order.
    pub(crate) rows: Vec<Row>,
    /// real line number -> display row (usize::MAX when hidden).
    pub(crate) real_row: Vec<usize>,
}

/// The display chars of one inline `⋯` marker (appended to its opening line).
pub(crate) const MARKER_CHARS: usize = 1;

impl FoldView {
    /// Real char index of the first displayed char of `real_line`; `None` when
    /// that line is hidden by a fold.
    pub(crate) fn char_of_real_line(&self, real_line: usize) -> Option<usize> {
        let row = *self.real_row.get(real_line)?;
        (row != usize::MAX).then(|| self.rows[row].char_start)
    }

    /// Display row of a real line, if visible.
    pub(crate) fn display_row_of(&self, real_line: usize) -> Option<usize> {
        let row = *self.real_row.get(real_line)?;
        (row != usize::MAX).then_some(row)
    }
}

pub(crate) fn build_fold_view(content: &str, folds: &[Fold]) -> FoldView {
    let chars: Vec<char> = content.chars().collect();
    let mut line_starts = vec![0usize];
    for (i, &c) in chars.iter().enumerate() {
        if c == '\n' {
            line_starts.push(i + 1);
        }
    }
    let line_of = |ci: usize| line_starts.partition_point(|&s| s <= ci).saturating_sub(1);

    // Effective folds, sorted: only innermost-per-line, non-nested ones that
    // are actually visible (an outer fold hides its inner folds' openings).
    let mut sorted: Vec<&Fold> = folds.iter().collect();
    sorted.sort_by_key(|f| (line_of(f.open), f.close));
    let mut eff: Vec<(usize, usize, usize)> = Vec::new(); // (open_line, close_line, open)
    for f in sorted {
        let (ol, cl) = (line_of(f.open), line_of(f.close));
        if ol >= cl {
            continue;
        }
        if eff.last().is_some_and(|&(po, pc, _)| ol == po || ol <= pc) {
            continue;
        }
        eff.push((ol, cl, f.open));
    }

    let n_lines = line_starts.len();
    let mut display = String::new();
    let mut d2r = Vec::with_capacity(chars.len() + eff.len() * 4 + 1);
    let mut rows: Vec<Row> = Vec::new();
    let mut real_row = vec![usize::MAX; n_lines];

    let mut count = 0usize; // chars pushed into `display`
    let mut li = 0usize;
    let mut fi = 0usize;
    while li < n_lines {
        // Consume every fold whose interior (including its closing line, which
        // is always hidden) now lies behind us — otherwise `fi` would stay
        // stuck on the first fold and a later sibling fold would never render.
        while eff.get(fi).is_some_and(|&(_, cl, _)| li > cl) {
            fi += 1;
        }
        let hides = eff
            .get(fi)
            .is_some_and(|&(ol, cl, _)| li > ol && li <= cl);
        if hides {
            li += 1;
            continue;
        }
        let char_start = count;
        let ls = line_starts[li];
        let le = line_starts.get(li + 1).map(|&s| s - 1).unwrap_or(chars.len());
        for ci in ls..le {
            d2r.push(ci);
            display.push(chars[ci]);
            count += 1;
        }
        // Mature fold: the `⋯` marker rides at the end of the opening line
        // itself (no extra row), so a folded block reads like VS Code's. The
        // marker maps to `open + 1` — just inside the fold's opening brace —
        // so typing at the marker lands right after the `{`.
        let marker = eff.get(fi).filter(|&&(ol, _, _)| ol == li).map(|&(_, _, o)| o);
        if let Some(open) = marker {
            d2r.push(open + 1);
            display.push_str(MARKER);
            count += MARKER_CHARS;
        }
        if li + 1 < n_lines {
            d2r.push(le);
            display.push('\n');
            count += 1;
        }
        real_row[li] = rows.len();
        let marker_char = marker.map(|_| count - MARKER_CHARS);
        rows.push(Row { line: li, char_start, marker_char, marker });
        li += 1;
    }
    d2r.push(chars.len());

    FoldView { display, d2r, rows, real_row }
}

/// Toggle the fold whose block starts on `real_line`: closes it when open,
/// opens it when already closed. Nested innermost block wins.
pub(crate) fn toggle_fold(
    folds: &mut Vec<Fold>,
    brace_opens: &[usize],
    content: &str,
    real_line: usize,
) {
    let line_of_char = |ci: usize| -> usize {
        let starts = line_starts_of(content);
        starts.partition_point(|&s| s <= ci).saturating_sub(1)
    };
    if folds.iter().any(|f| line_of_char(f.open) == real_line) {
        folds.retain(|f| line_of_char(f.open) != real_line);
        return;
    }
    if let Some(&open) = brace_opens
        .iter()
        .filter(|&&o| line_of_char(o) == real_line)
        .max()
    {
        let close = closest_close(content, open);
        if let Some(close) = close {
            if line_of_char(close) > real_line {
                folds.push(Fold { open, close });
            }
        }
    }
}

/// Unfold every fold containing `real_line` in its hidden interior (used by
/// jump-to-line when the target line is hidden).
pub(crate) fn unfold_covering(folds: &mut Vec<Fold>, content: &str, real_line: usize) {
    let line_of_char = |ci: usize| -> usize {
        let starts = line_starts_of(content);
        starts.partition_point(|&s| s <= ci).saturating_sub(1)
    };
    folds.retain(|f| {
        let (ol, cl) = (line_of_char(f.open), line_of_char(f.close));
        !(ol < real_line && real_line <= cl)
    });
}

/// Display rows that carry a fold toggle: the opening line of every closed
/// fold plus every visible line where a multi-line brace block starts.
pub(crate) fn fold_rows(
    view: &FoldView,
    folds: &[Fold],
    brace_opens: &[usize],
    content: &str,
) -> Vec<usize> {
    let line_of_char = |ci: usize| -> usize {
        let starts = line_starts_of(content);
        starts.partition_point(|&s| s <= ci).saturating_sub(1)
    };
    let mut set = std::collections::BTreeSet::new();
    for f in folds {
        if let Some(r) = view.display_row_of(line_of_char(f.open)) {
            set.insert(r);
        }
    }
    for &o in brace_opens {
        if let Some(r) = view.display_row_of(line_of_char(o)) {
            set.insert(r);
        }
    }
    set.into_iter().collect()
}

// ---------------------------------------------------------------------------
// FoldBuffer: the TextBuffer that TextEdit sees.
// ---------------------------------------------------------------------------

pub(crate) struct FoldBuffer<'a> {
    content: &'a mut String,
    folds: &'a mut Vec<Fold>,
    view: FoldView,
}

impl<'a> FoldBuffer<'a> {
    pub(crate) fn new(content: &'a mut String, folds: &'a mut Vec<Fold>) -> Self {
        let view = build_fold_view(content, folds);
        Self { content, folds, view }
    }

    pub(crate) fn view(&self) -> &FoldView {
        &self.view
    }

    fn rebuild(&mut self) {
        self.view = build_fold_view(self.content, self.folds);
    }

    /// Display char index -> real char index, clamped to the real end.
    fn to_real(&self, di: usize) -> usize {
        let n = self.view.display.chars().count();
        self.view.d2r[di.min(n)]
    }

    /// Open-brace indices of every fold whose inline `⋯` marker intersects (or
    /// is touched by) `[s,e)` — typing right at the marker's spot unfolds.
    fn markers_in(&self, s: usize, e: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for r in &self.view.rows {
            let (Some(open), Some(mc)) = (r.marker, r.marker_char) else {
                continue;
            };
            let hi = mc + MARKER_CHARS - 1;
            if s <= hi && e >= mc {
                out.push(open);
            }
        }
        out
    }

    fn unfold(&mut self, opens: &[usize]) {
        if opens.is_empty() {
            return;
        }
        self.folds.retain(|f| !opens.contains(&f.open));
        self.rebuild();
    }

    fn splice(&mut self, start: usize, end: usize, text: &str) {
        // Rare: a marker-context edit can make a fold's stored braces stale for
        // one frame; the editor re-prunes folds against fresh brace pairs each
        // frame. The file itself is always edited exactly as requested.
        let mut chars: Vec<char> = self.content.chars().collect();
        let end = end.min(chars.len()).max(start);
        chars.splice(start..end, text.chars());
        *self.content = chars.into_iter().collect();
        self.rebuild();
    }
}

impl TextBuffer for FoldBuffer<'_> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        &self.view.display
    }

    fn insert_text(&mut self, text: &str, char_index: CharIndex) -> usize {
        let di = char_index.0;
        // Unfold FIRST, then map the edit onto the unfolded text: the user
        // always sees exactly what the edit touches — real hidden content is
        // never destroyed. The marker anchors at `open + 1`, so typing at the
        // `⋯` lands right after the opening `{`.
        let opens = self.markers_in(di, di + 1);
        self.unfold(&opens);
        let ri = self.to_real(di);
        self.splice(ri, ri, text);
        text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: Range<CharIndex>) {
        let (s, e) = (char_range.start.0, char_range.end.0);
        // Same order as insert_text: unfold, then map against the honest view.
        let opens = self.markers_in(s, e);
        self.unfold(&opens);
        let (rs, re) = (self.to_real(s), self.to_real(e));
        if rs < re {
            self.splice(rs, re, "");
        }
    }

    fn type_id(&self) -> std::any::TypeId {
        // FoldBuffer borrows the text, so `TypeId::of::<Self>()` would need
        // `'static`; no one ever downcasts — a stable tag is enough.
        TypeId::of::<FoldBufferTag>()
    }
}

/// Zero-sized stand-in for [`FoldBuffer`]'s `type_id` (can't name the real type
/// in `TypeId::of` without `'static`).
struct FoldBufferTag;

// ---------------------------------------------------------------------------
// FoldIcon: chevron SVGs in the gutter on rows with a collapsed fold (`up`,
// click to expand) and on open foldable blocks (`down`, click to collapse).
// ---------------------------------------------------------------------------

const ICON_PX: f32 = 16.0;

pub(crate) fn draw_fold_icons(
    ui: &egui::Ui,
    icons: &mut Icons,
    rows: &[usize],
    collapsed: &std::collections::HashSet<usize>,
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    icon_x: f32,
    clicked: &mut Vec<usize>,
) {
    for &row in rows {
        let Some(grow) = galley.rows.get(row) else {
            continue;
        };
        let y = galley_pos.y + grow.pos.y + (grow.size.y - ICON_PX) / 2.0;
        let rect = egui::Rect::from_min_size(
            egui::Pos2::new(icon_x, y),
            egui::vec2(ICON_PX, ICON_PX),
        );
        // Closed fold -> chevron up (expand it), open foldable block -> chevron
        // down (collapse it). Rasterized once and cached by the Icons cache.
        let icon = if collapsed.contains(&row) {
            Icon::ChevronUp
        } else {
            Icon::ChevronDown
        };
        let tex = icons.texture(ui.ctx(), icon);
        ui.painter().image(
            tex.id(),
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::from_gray(140),
        );
        let resp = ui.interact(rect, egui::Id::new(("fold_icon", row)), egui::Sense::click());
        if resp.clicked() {
            clicked.push(row);
        }
    }
}

// ---------------------------------------------------------------------------
// Small shared helpers.
// ---------------------------------------------------------------------------

fn line_starts_of(content: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, c) in content.chars().enumerate() {
        if c == '\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Which `{}` pairs are foldable, judged by their *shape* on the source line.
///
/// A brace block is only a fold target when it looks like one:
/// - the opening `{` is the last non-blank character of its line
///   (`fn a() {`, `if x {`) — this rules out inline expression braces like
///   `foo({ .. })` and `let x = Foo { a: 1 };`; and
/// - nothing but a block continuation follows the closing `}` on its line
///   (bare `}`, `} else {`, `} while …`). A `;`,`,`/`)`/`]` after the `}` means
///   a multi-line *literal* is closing (`Foo {\n …\n};`, `vec!{…},`), which
///   must not fold.
pub(crate) fn foldable_opens(content: &str, braces: &[BracePair]) -> Vec<usize> {
    let chars: Vec<char> = content.chars().collect();
    let mut out = Vec::new();
    for b in braces {
        if b.close == usize::MAX {
            continue;
        }
        let tail_open: Vec<char> = chars
            .iter()
            .skip(b.open + 1)
            .take_while(|&&c| c != '\n')
            .cloned()
            .collect();
        if tail_open.iter().any(|c| !c.is_whitespace()) {
            continue;
        }
        let tail_close: Vec<char> = chars
            .iter()
            .skip(b.close + 1)
            .take_while(|&&c| c != '\n')
            .cloned()
            .collect();
        let after = tail_close.iter().find(|c| !c.is_whitespace());
        if after.is_some_and(|&c| matches!(c, ';' | ',' | ')' | ']' | '>' | '}' | '.' | '=' | ':' | '<')) {
            continue;
        }
        out.push(b.open);
    }
    out
}

/// The real char index right after `open` where the matching `}` sits, walking
/// one mask-free brace depth (strings/comments skipped).
fn closest_close(content: &str, open: usize) -> Option<usize> {
    let chars: Vec<char> = content.chars().collect();
    let mut depth = 0usize;
    for (i, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                if depth == 1 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    /// Two sibling folds (independent functions) must fold at the same time —
    /// one closed fold must never disable the others.
    #[test]
    fn sibling_folds_coexist() {
        let content = "fn a() {\n    x();\n}\nfn b() {\n    y();\n}\n";
        //             012345678 9..16 17 18 19 20..27 28 29..36 37 38 39
        let folds = vec![Fold { open: 7, close: 18 }, Fold { open: 27, close: 38 }];
        let view = build_fold_view(content, &folds);
        let markers: Vec<Option<usize>> = view.rows.iter().map(|r| r.marker).collect();
        // Rows: line 0 (fn a, marker), line 3 (fn b, marker), trailing empty.
        assert_eq!(markers[0], Some(7));
        assert_eq!(markers[1], Some(27));
        assert_eq!(view.display, "fn a() {⋯\nfn b() {⋯\n");
    }

    /// Only well-formed `{ ... }` blocks fold: fn bodies, if/else — but never
    /// multi-line expression literals (`Foo {\n …\n};`) or inline braces.
    #[test]
    fn foldable_opens_shape_filter() {
        let content = "fn a() {\n    x();\n}\nlet m = Foo {\n    a: 1,\n};\nlet z = Bar { a: 1 };\nif x {\n    y();\n} else {\n    z();\n}\n";
        let chars: Vec<char> = content.chars().collect();
        let braces = crate::guides::analyze_brackets(&chars, &vec![false; chars.len()]).brace_pairs;
        // The two expression literals (`Foo {\n …\n};`, `Bar { a: 1 };`) must
        // be filtered out; the fn body and the if/else blocks must survive.
        let mut literal_opens: Vec<usize> = braces
            .iter()
            .filter(|b| b.open > 8)
            .take(2)
            .map(|b| b.open)
            .collect();
        literal_opens.sort_unstable();
        let raw: Vec<usize> = braces.iter().map(|b| b.open).collect();
        let expected: Vec<usize> = raw
            .into_iter()
            .filter(|o| !literal_opens.contains(o))
            .collect();
        assert_eq!(foldable_opens(content, &braces), expected);
        assert!(expected.len() >= 3);
    }

    /// The `⋯` marker must ride on the opening line itself (no extra row), so
    /// a folded file keeps line numbers 1:1 with reality.
    #[test]
    fn folded_display_uses_inline_marker() {
        let content = "fn a() {\n    x();\n}\nfn b() {\n    y();\n}\n";
        let folds = vec![Fold { open: 27, close: 35 }];
        let view = build_fold_view(content, &folds);
        assert_eq!(view.display, "fn a() {\n    x();\n}\nfn b() {⋯\n}\n");
        // Lines 0..3, the fold's closing line 5 and the trailing empty line.
        assert_eq!(view.rows.len(), 6);
        assert_eq!(view.rows[3].line, 3);
        assert_eq!(view.rows[3].marker, Some(27));
        assert_eq!(view.rows[3].marker_char, Some(29));
        assert_eq!(view.display_row_of(4), None);
        assert_eq!(view.display_row_of(5), Some(4));
    }

    /// The inline marker is anchored at `open + 1`, so its display range is
    /// `[28, 29)` for the fold at open 27.
    #[test]
    fn editing_the_marker_unfolds() {
        let content = "fn a() {\n    x();\n}\nfn b() {\n    y();\n}\n";
        let original = content.to_string();

        // Inserting at the marker char: unfolds, then maps onto the unfolded
        // text — '!' goes right after the opening '{'.
        let mut content = original.clone();
        let mut folds = vec![Fold { open: 27, close: 35 }];
        let mut fb = FoldBuffer::new(&mut content, &mut folds);
        assert_eq!(fb.insert_text("!", egui::text::CharIndex(28)), 1);
        assert!(folds.is_empty());
        assert!(content.contains("fn b() {!"));

        // Deleting exactly the marker char: unfolds and removes the (visible)
        // newline that the marker showed — nothing hidden is ever destroyed.
        let mut content = original.clone();
        let mut folds = vec![Fold { open: 27, close: 35 }];
        let mut fb = FoldBuffer::new(&mut content, &mut folds);
        fb.delete_char_range(egui::text::CharIndex(28)..egui::text::CharIndex(29));
        assert!(folds.is_empty());
        assert_eq!(content, "fn a() {\n    x();\n}\nfn b() {    y();\n}\n");
    }
}
