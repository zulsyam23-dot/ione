//! Pointer/cursor-aware lookup: the active pair for the text caret, plus
//! hover-activation so moving the mouse over a guide lights it up without a
//! click.

use eframe::egui::{self, Pos2};

use crate::guides::geometry::{char_line, char_rect, pair_guide_x};

use super::{BracketScan, Pair};

/// The innermost pair whose guide column the pointer is touching. Only pairs
/// with a drawn guide (multi-line) count; pairs are sorted by `open`, so a
/// later match is more deeply nested and simply overwrites — innermost wins.
pub(crate) fn hovered_pair(
    pointer: Pos2,
    galley: &egui::Galley,
    origin: Pos2,
    pairs: &[Pair],
) -> Option<Pair> {
    const TOL: f32 = 6.0;
    let mut best: Option<Pair> = None;
    for p in pairs.iter().filter(|p| p.close != usize::MAX) {
        if char_line(galley, p.open) == char_line(galley, p.close) {
            continue;
        }
        let Some(x) = pair_guide_x(galley, origin, *p) else {
            continue;
        };
        if (pointer.x - x).abs() > TOL {
            continue;
        }
        let open_r = char_rect(galley, origin, p.open);
        let close_r = char_rect(galley, origin, p.close);
        if open_r.bottom() <= pointer.y && pointer.y <= close_r.top() {
            best = Some(*p);
        }
    }
    best
}

impl BracketScan {
    /// The innermost pair that encloses the cursor (including a cursor sitting
    /// on the closing bracket). Unmatched open brackets count too.
    ///
    /// Pairs are sorted by `open`, so a binary-search split on `open < cursor`
    /// leaves only candidates that could still be open at the cursor; walking
    /// them backwards visits innermost-first. Cost = nesting depth at the
    /// cursor, not the total pair count.
    pub(crate) fn active_pair(&self, cursor: usize) -> Option<Pair> {
        let split = self.pairs.partition_point(|p| p.open < cursor);
        for p in self.pairs[..split].iter().rev() {
            if p.close == usize::MAX || p.close >= cursor {
                return Some(*p);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guides::brackets::scanner::analyze_brackets;

    fn scan_of(content: &str) -> BracketScan {
        analyze_brackets(&content.chars().collect::<Vec<char>>(), &[])
    }

    #[test]
    fn active_pair_encloses_cursor() {
        let content = "outer(inner[deep])";
        let scan = scan_of(content);
        let deep_open = content.find('[').unwrap();
        let deep_close = content.find(']').unwrap();
        let inner_open = content.find('(').unwrap();
        let inner_close = content.find(')').unwrap();

        // Cursor right after the inner open bracket, before the nested `[`, is
        // inside the `()` pair only.
        let active_outer = scan.active_pair(inner_open + 1).unwrap();
        assert_eq!((active_outer.open, active_outer.close), (inner_open, inner_close));

        // Cursor inside the nested `[]` picks the innermost pair.
        let active_deep = scan.active_pair(deep_open + 1).unwrap();
        assert_eq!((active_deep.open, active_deep.close), (deep_open, deep_close));

        // Cursor right on the closing bracket still counts as inside.
        let active_at_close = scan.active_pair(inner_close).unwrap();
        assert_eq!((active_at_close.open, active_at_close.close), (inner_open, inner_close));

        // Cursor before the opening bracket is not inside.
        assert!(scan.active_pair(inner_open).is_none());
    }

    #[test]
    fn unmatched_open_is_active_anywhere_below() {
        let scan = scan_of("if (x {");
        let open = 3;
        // Unmatched `{` stays active for any cursor after it.
        assert!(scan.active_pair(open + 1).is_some());
        assert!(scan.active_pair(1000).is_some());
    }

    #[test]
    fn hovering_a_guide_activates_innermost_pair() {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        let content = "fn main() {\n    if x {\n        a();\n    }\n}\n";
        let origin = Pos2::ZERO;

        let galley: std::cell::RefCell<Option<std::sync::Arc<egui::Galley>>> = std::cell::RefCell::new(None);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0))),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let g = ui.fonts_mut(|f| f.layout_job(egui::text::LayoutJob::single_section(
                    content.to_string(),
                    egui::TextFormat::simple(egui::FontId::monospace(14.0), egui::Color32::WHITE),
                )));
                *galley.borrow_mut() = Some(g);
            });
        });
        output.drop_without_applying_deltas();
        let galley = galley.into_inner().unwrap();

        let content_chars: Vec<char> = content.chars().collect();
        let scan = analyze_brackets(&content_chars, &[]);

        let mid_y = galley.rows.iter().map(|r| r.size.y).sum::<f32>() / 2.0;

        // Pointer on the level-2 column (inner `})` closes at 4 chars) hits the
        // innermost `if` pair, not the enclosing `fn` pair.
        let lvl1_x = char_rect(&galley, origin, 4).left();
        let inner = hovered_pair(Pos2::new(lvl1_x, mid_y), &galley, origin, &scan.pairs)
            .expect("hovering the inner guide should return the if-pair");
        let if_open = content.find("if x {").unwrap() + 5;
        assert_eq!(inner.open, if_open);

        // Pointer on the margin column (outer `}` closes at column 0) hits the
        // `fn` pair only.
        let outer = hovered_pair(Pos2::new(2.0, mid_y), &galley, origin, &scan.pairs)
            .expect("hovering the outer guide should return the fn-pair");
        assert_eq!(outer.open, content.find('{').unwrap());

        // Pointer far from any column yields nothing.
        assert!(hovered_pair(Pos2::new(lvl1_x + 40.0, mid_y), &galley, origin, &scan.pairs).is_none());
        // Single-line pair (no guide) is never hoverable.
        let one_line = "fn f() { g(); }\n";
        let g: std::cell::RefCell<Option<std::sync::Arc<egui::Galley>>> = std::cell::RefCell::new(None);
        let input2 = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 200.0))),
            ..Default::default()
        };
        let output2 = ctx.run_ui(input2, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let gal = ui.fonts_mut(|f| f.layout_job(egui::text::LayoutJob::single_section(
                    one_line.to_string(),
                    egui::TextFormat::simple(egui::FontId::monospace(14.0), egui::Color32::WHITE),
                )));
                *g.borrow_mut() = Some(gal);
            });
        });
        output2.drop_without_applying_deltas();
        let g = g.into_inner().unwrap();
        let s2 = analyze_brackets(&one_line.chars().collect::<Vec<char>>(), &[]);
        assert!(hovered_pair(Pos2::new(4.0, 4.0), &g, origin, &s2.pairs).is_none());
    }
}