//! Built-in error/warning diagnostics: a pure per-language analysis producing
//! single-line [`Diagnostic`] ranges (real-buffer char indices), rendered as
//! squiggles + gutter markers by the editor.
//!
//! The analysis is deliberately conservative — only checks with no reasonable
//! false positives (mask-aware bracket balance, trailing whitespace, mixed
//! indentation). Everything hotter (real type errors, LSP) slots in later at
//! this single `analyze` entry point; the renderer never changes.

use eframe::egui::{self, Rect, Stroke};
use egui_code_editor::Syntax;

use crate::editor::folds::FoldView;
use crate::guides::brackets::BracketScan;
use crate::guides::geometry::char_rect;
use crate::guides::analyze_brackets;
use crate::style::Palette;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    Warning,
}

/// A single-line problem in the real buffer, `start..end` exclusive in CHAR
/// indices (matches `FoldView`/cursor space, so folds map directly to display).
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub start: usize,
    pub end: usize,
    pub message: String,
}

impl Diagnostic {
    fn error(start: usize, end: usize, message: impl Into<String>) -> Self {
        Self { severity: Severity::Error, start, end, message: message.into() }
    }
    fn warning(start: usize, end: usize, message: impl Into<String>) -> Self {
        Self { severity: Severity::Warning, start, end, message: message.into() }
    }
}

/// Analyze `content` with the given syntax and return sorted diagnostics plus
/// the mask-aware bracket scan (the scanner runs once and is shared with the
/// editor overlay cache).
pub fn analyze_with_scan(content: &str, syntax: &Syntax) -> (Vec<Diagnostic>, BracketScan) {
    let mut out = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let (mask, _) = crate::editor::styling::mask_and_links(content, syntax);
    let scan = analyze_brackets(&chars, &mask);

    // Unmatched opens: `(`, `[`, `{` at end of the scan.
    for p in &scan.pairs {
        if p.close == usize::MAX {
            let ch = chars.get(p.open).copied().unwrap_or('?');
            out.push(Diagnostic::error(p.open, p.open + 1, format!("unclosed bracket `{ch}`")));
        }
    }
    // Stray closes: `)`, `]`, `}` with no matching open.
    for &ci in &scan.unmatched_closes {
        let ch = chars.get(ci).copied().unwrap_or('?');
        out.push(Diagnostic::error(ci, ci + 1, format!("unexpected closing bracket `{ch}`")));
    }

    // Per-line checks: trailing whitespace and mixed indentation. Blank lines
    // are skipped for trailing whitespace (all-whitespace lines are indent).
    let mut idx = 0usize; // char index of line start in the real buffer
    for piece in content.split_inclusive('\n') {
        let line = piece.strip_suffix('\n').unwrap_or(piece);
        let lc: Vec<char> = line.chars().collect();
        let n = lc.len();

        let ws = lc.iter().rposition(|c| *c != ' ' && *c != '\t').map(|i| i + 1).unwrap_or(0);
        if ws > 0 && ws < n && lc[..ws].iter().any(|c| *c != ' ' && *c != '\t') {
            out.push(Diagnostic::warning(idx + ws, idx + n, "trailing whitespace"));
        }

        let lead = lc.iter().take_while(|c| **c == ' ' || **c == '\t').count();
        if lead > 0 && lc[..lead].contains(&'\t') && lc[..lead].contains(&' ') {
            out.push(Diagnostic::warning(idx, idx + lead, "mixed indentation (tabs and spaces)"));
        }

        idx += piece.chars().count();
    }

    out.sort_by_key(|d| d.start);
    (out, scan)
}

/// Draw the squiggle under every visible diagnostic and, when the pointer
/// hovers one, show its message tooltip. Ranges are mapped real → display via
/// the fold view; anything hidden behind a fold is skipped. All diagnostics
/// are single-line, so each squiggle stays on one row (never crossing a wrap).
pub fn draw_squiggles(
    ui: &egui::Ui,
    editor_id: &str,
    galley: &egui::Galley,
    origin: egui::Pos2,
    view: &FoldView,
    diags: &[Diagnostic],
    palette: &Palette,
) {
    if diags.is_empty() {
        return;
    }
    let painter = ui.painter();
    let mut hovered: Option<&Diagnostic> = None;
    let to_display = |real: usize| view.d2r.iter().position(|&r| r == real);

    for d in diags {
        let Some(ds) = to_display(d.start) else { continue };
        let Some(de) = to_display(d.end)
            .or_else(|| (d.end > d.start).then(|| to_display(d.end - 1)).flatten())
            .filter(|&de| de >= ds)
        else {
            continue;
        };
        let left = char_rect(galley, origin, ds).left();
        let right = char_rect(galley, origin, de).left();
        if right - left < 1.0 {
            continue;
        }
        let color = match d.severity {
            Severity::Error => palette.diag_error,
            Severity::Warning => palette.diag_warning,
        };
        let row_rect = char_rect(galley, origin, ds);
        // Zigzag: alternating two-segment teeth, kept under the last baseline.
        let y = row_rect.bottom() - 2.0;
        const AMP: f32 = 1.5;
        const STEP: f32 = 4.0;
        let stroke = Stroke::new(1.2, color);
        let mut x = left;
        let mut up = true;
        while x + STEP < right {
            let p1 = egui::pos2(x, y + if up { AMP } else { 0.0 });
            let p2 = egui::pos2(x + STEP, y + if up { 0.0 } else { AMP });
            painter.line_segment([p1, p2], stroke);
            x += STEP;
            up = !up;
        }
        if right - x >= 1.0 {
            let p1 = egui::pos2(x, y + if up { AMP } else { 0.0 });
            let p2 = egui::pos2(right, y + if up { 0.0 } else { AMP });
            painter.line_segment([p1, p2], stroke);
        }

        // Hover detection in the same screen-space as the painted squiggles.
        if hovered.is_none() {
            let rect = Rect::from_min_max(
                egui::pos2(left, row_rect.top()),
                egui::pos2(right, row_rect.bottom()),
            );
            if ui.ctx().pointer_hover_pos().is_some_and(|p| rect.contains(p)) {
                hovered = Some(d);
            }
        }
    }

    if let Some(d) = hovered {
        let sev_color = match d.severity {
            Severity::Error => palette.diag_error,
            Severity::Warning => palette.diag_warning,
        };
        let pos = ui.ctx().pointer_hover_pos().unwrap_or_default();
        egui::Area::new(egui::Id::new(("diag_tooltip", editor_id)))
            .order(egui::Order::Tooltip)
            .fixed_pos(pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(match d.severity {
                                Severity::Error => "error",
                                Severity::Warning => "warning",
                            })
                            .color(sev_color)
                            .strong()
                            .size(12.0),
                        );
                        ui.label(egui::RichText::new(&d.message).color(palette.text).size(12.5));
                    });
            });
    }
}

/// Real line number of a char index in `content`.
pub fn line_of_char(content: &str, char_idx: usize) -> usize {
    content.chars().take(char_idx).filter(|&c| c == '\n').count()
}

/// Cheap content hash; a changed hash is the trigger for `analyze`.
pub fn hash_content(content: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in content.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msgs(content: &str, syntax: Option<Syntax>) -> Vec<(Severity, String)> {
        analyze_with_scan(content, &syntax.unwrap_or_else(Syntax::rust)).0
            .into_iter()
            .map(|d| (d.severity, d.message))
            .collect()
    }

    #[test]
    fn unmatched_brackets_are_errors() {
        let out = msgs("fn f() {\n    let x = (1;\n}\n", None);
        assert!(out.iter().any(|(s, m)| *s == Severity::Error && m.contains("(")));
    }

    #[test]
    fn stray_closing_bracket_is_an_error() {
        let out = msgs("let x = 1);\n", None);
        assert!(out.iter().any(|(s, m)| *s == Severity::Error && m.contains(")")));
    }

    #[test]
    fn balanced_brackets_cause_no_bracket_errors() {
        let out = msgs("fn f(a: [i32; 3]) {\n    // (comment)\n    let s = \"unclosed? no)\";\n}\n", None);
        assert!(!out.iter().any(|(s, _)| *s == Severity::Error));
    }

    #[test]
    fn strings_and_comments_do_not_feed_bracket_errors() {
        // `)` inside a string and `}` inside a comment must not count.
        let out = msgs("let s = \")\"; // }\n", None);
        assert!(!out.iter().any(|(s, _)| *s == Severity::Error));
    }

    #[test]
    fn trailing_whitespace_is_a_warning_with_line_range() {
        let content = "let a = 1;   \nlet b = 2;\n";
        let d = analyze_with_scan(content, &Syntax::rust()).0;
        let tws: Vec<&Diagnostic> = d.iter().filter(|d| d.message.contains("trailing")).collect();
        assert_eq!(tws.len(), 1);
        assert_eq!(tws[0].start, 10); // chars 10..13 are the three trailing spaces
        assert_eq!(tws[0].end, 13);
        assert_eq!(&content[tws[0].start..tws[0].end], "   ");
        // Fully-blank line: no trailing-whitespace warning.
        let d = analyze_with_scan("a();\n     \nb();\n", &Syntax::rust()).0;
        assert!(d.iter().all(|d| !d.message.contains("trailing")));
    }

    #[test]
    fn mixed_indentation_is_a_warning() {
        let out = msgs("if a {\n\t  let x = 1;\n}\n", None);
        assert!(out.iter().any(|(s, m)| *s == Severity::Warning && m.contains("mixed")));
        // Pure tabs or pure spaces: clean.
        let out = msgs("if a {\n\tlet x = 1;\n      let y = 2;\n}\n", None);
        assert!(!out.iter().any(|(_, m)| m.contains("mixed")));
    }

    #[test]
    fn diagnostics_are_sorted() {
        let content = "if a {\n\t  x(;\n\nb(;\n   \n}\n".to_string();
        let d = analyze_with_scan(&content, &Syntax::rust()).0;
        for w in d.windows(2) {
            assert!(w[0].start <= w[1].start);
        }
    }
}