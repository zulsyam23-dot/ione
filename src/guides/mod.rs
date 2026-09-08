//! Editor overlays: bracket-pair guides and the active-pair highlight.
//! `draw_editor_overlays` is the single entry point, called from the editor's
//! scroll-content `Ui` after `TextEdit::show`.

//! Submodules, each a single responsibility:
//! - `brackets`: mask-aware bracket scan (pairs, depths, active pair).
//! - `geometry`: galley pixel math and the low-level guide strokes.

use egui::text::CCursorRange;
use eframe::egui::{self, Id, Pos2};

use crate::style::Palette;

pub(crate) mod brackets;
pub(crate) mod geometry;

pub(crate) use brackets::{analyze_brackets, hovered_pair, BracketScan, Pair};

use geometry::{char_rect, draw_pair_guide, guide_line, pair_guide_x};

#[derive(Clone, Copy)]
pub struct EditorOverlay {
    pub bracket_guides: bool,
    pub colorize_brackets: bool,
}

/// Draw bracket guides on top of the rendered editor. `ui` must be the
/// editor's scroll-content `Ui`: its painter is clipped to the viewport and
/// translated with the scroll, so the guides always overlay the glyphs exactly
/// and never bleed into the surrounding ui. `galley`/`galley_pos`/`cursor_range`
/// come straight from the `TextEdit` (the layouter output for this frame).
pub fn draw_editor_overlays(
    ui: &egui::Ui,
    editor_id: &str,
    galley: &egui::Galley,
    galley_pos: Pos2,
    cursor_range: Option<CCursorRange>,
    scan: &BracketScan,
    overlay: &EditorOverlay,
    palette: &Palette,
    font_size: f32,
) {
    let cursor = cursor_range.map(|r| r.primary.index.0);
    // Hover takes precedence: moving the mouse over a guide activates that
    // pair with no click; fall back to the text caret otherwise.
    let active = ui
        .input(|i| i.pointer.hover_pos())
        .and_then(|p| hovered_pair(p, galley, galley_pos, &scan.pairs))
        .or_else(|| cursor.and_then(|c| scan.active_pair(c)));

    let painter = ui.painter();
    let origin = galley_pos;
    let pairs = &scan.pairs;

    if overlay.bracket_guides {
        // First (outermost, depth-0) guide keeps the full width; every deeper
        // level draws thinner so shrinking blocks don't stack up into a solid
        // bar. Static and active highlight share the same width per pair.
        let width_for = |p: &Pair| {
            let depth = scan.depths.get(p.open).copied().unwrap_or(0);
            if depth == 0 {
                font_size * 0.18
            } else {
                font_size * 0.10
            }
        };
        let color = palette.bracket_guide;
        for p in pairs.iter().filter(|p| p.close != usize::MAX) {
            if pair_guide_x(galley, origin, *p).is_some() {
                draw_pair_guide(&painter, galley, origin, *p, width_for(p), color);
            }
        }
        // Active-pair highlight fades in/out instead of blinking on/off. Two
        // egui quirks: `animate_bool` snaps to the target on the FIRST call
        // for an id (animation_manager.rs), and repaints stop once input
        // stops. So per pair we prime the animator to 0 on first sight and
        // force repaints mid-fade. Keying the animator id by the pair's open
        // index makes every pair change restart the ramp instead of leaving
        // the previous pair's high value in place.
        const FADE: f32 = 0.3;
        let fade_id = Id::new(("active_guide_fade", editor_id));
        let last_open = ui
            .ctx()
            .data_mut(|d| d.get_temp::<i64>(fade_id.with("last_pair")))
            .unwrap_or(-1);
        let key: i64 = active.map_or(last_open, |p| p.open as i64);
        let anim_id = fade_id.with(("pair", key));
        if key != last_open {
            ui.ctx().animate_bool_with_time(anim_id, false, FADE);
            ui.ctx().data_mut(|d| d.insert_temp(fade_id.with("last_pair"), key));
        }
        let t = ui.ctx().animate_bool_with_time(anim_id, active.is_some(), FADE);
        if t > 0.0 && t < 1.0 {
            ui.ctx().request_repaint();
        }
        if let Some(p) = active.filter(|_| t > 0.0) {
            // smoothstep: slow near both ends, fastest mid-way.
            let e = t * t * (3.0 - 2.0 * t);
            // Grow from the vertical middle toward both ends instead of
            // popping in as a full line.
            if let Some(x) = pair_guide_x(galley, origin, p) {
                let open_r = char_rect(galley, origin, p.open);
                let close_r = char_rect(galley, origin, p.close);
                let y0 = open_r.bottom().round();
                let y1 = close_r.top().round();
                if y1 > y0 {
                    let mid = (y0 + y1) / 2.0;
                    let half_height = (y1 - y0) / 2.0 * e;
                    guide_line(
                        &painter,
                        x,
                        mid - half_height,
                        mid + half_height,
                        width_for(&p),
                        palette.bracket_active.gamma_multiply(0.95 * e),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::text::LayoutJob;

    fn guide_rects(content: &str) -> Vec<(f32, f32, f32)> {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        let origin = egui::Pos2::new(4.0, 2.0);
        let overlay = EditorOverlay { bracket_guides: true, colorize_brackets: false };
        let scan = analyze_brackets(&content.chars().collect::<Vec<char>>(), &[]);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 300.0))),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let galley = ui.fonts_mut(|f| f.layout_job(LayoutJob::single_section(
                    content.to_string(),
                    egui::TextFormat::simple(egui::FontId::monospace(14.0), egui::Color32::WHITE),
                )));
                draw_editor_overlays(ui, "guide_test", &galley, origin, None, &scan, &overlay, &Palette::dark(), 14.0);
            });
        });
        let mut painted = Vec::new();
        for cs in &output.shapes {
            if let egui::Shape::Rect(r) = &cs.shape {
                painted.push((r.rect.left(), r.rect.top(), r.rect.bottom()));
            }
        }
        output.drop_without_applying_deltas();
        painted
    }

    #[test]
    fn left_margin_bracket_guide_is_drawn() {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        let content = "fn main() {\n    a();\n    b();\n    c();\n}\n";
        let origin = egui::Pos2::new(4.0, 2.0);
        let scan = analyze_brackets(&content.chars().collect::<Vec<char>>(), &[]);
        let overlay = EditorOverlay { bracket_guides: true, colorize_brackets: false };
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 300.0))),
            ..Default::default()
        };
        let expected_x: std::cell::Cell<f32> = std::cell::Cell::new(0.0);
        let output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let galley = ui.fonts_mut(|f| f.layout_job(LayoutJob::single_section(
                    content.to_string(),
                    egui::TextFormat::simple(egui::FontId::monospace(14.0), egui::Color32::WHITE),
                )));
                // `fn main`'s `}` closes at the left margin; its guide frames
                // the body right at the edge like any other multi-line pair.
                expected_x.set(char_rect(&galley, origin, content.find('}').unwrap()).left().round());
                draw_editor_overlays(ui, "guide_test", &galley, origin, None, &scan, &overlay, &Palette::dark(), 14.0);
            });
        });
        let mut tall = Vec::new();
        for cs in &output.shapes {
            if let egui::Shape::Rect(r) = &cs.shape {
                if r.rect.height() > 20.0 {
                    tall.push(r.rect.center().x);
                }
            }
        }
        output.drop_without_applying_deltas();
        assert!(tall.iter().any(|&cx| (cx - expected_x.get()).abs() <= 1.0), "margin guide must be drawn: {tall:?}");
    }

    #[test]
    fn nested_blocks_get_one_guide_per_close_column() {
        // Each multi-line pair draws a guide centered on its own closing
        // bracket's column; the outermost `fn` closes at the margin (dropped),
        // so just the two inner closes remain.
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        let content = "fn main() {\n    if x {\n        for i in 0..3 {\n            a();\n        }\n    }\n}\n";
        let origin = egui::Pos2::new(0.0, 0.0);
        let scan = analyze_brackets(&content.chars().collect::<Vec<char>>(), &[]);
        let overlay = EditorOverlay { bracket_guides: true, colorize_brackets: false };
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 400.0))),
            ..Default::default()
        };
        let if_x: std::cell::Cell<f32> = std::cell::Cell::new(0.0);
        let for_x: std::cell::Cell<f32> = std::cell::Cell::new(0.0);
        let output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let galley = ui.fonts_mut(|f| f.layout_job(LayoutJob::single_section(
                    content.to_string(),
                    egui::TextFormat::simple(egui::FontId::monospace(14.0), egui::Color32::WHITE),
                )));
                // Close `}` of the `if` block at col 4, of the `for` block at
                // col 8. The guide must be centered on exactly those columns.
                let if_close = content.find("    }\n").unwrap() + 4;
                let for_close = content.find("        }\n").unwrap() + 8;
                if_x.set(char_rect(&galley, origin, if_close).left().round());
                for_x.set(char_rect(&galley, origin, for_close).left().round());
                draw_editor_overlays(ui, "guide_test", &galley, origin, None, &scan, &overlay, &Palette::dark(), 14.0);
            });
        });
        let mut painted = Vec::new();
        for cs in &output.shapes {
            if let egui::Shape::Rect(r) = &cs.shape {
                if r.rect.width() <= 50.0 {
                    painted.push(r.rect.center().x);
                }
            }
        }
        output.drop_without_applying_deltas();
        // `fn main` closes at the margin, `if` at col 4, `for` at col 8 — all
        // three multi-line pairs draw one guide on their own close column.
        assert_eq!(painted.len(), 3, "expected fn + if + for guides: {painted:?}");
        assert!(painted.iter().any(|&x| (x - if_x.get()).abs() <= 1.0), "missing col-4 guide: {painted:?}");
        assert!(painted.iter().any(|&x| (x - for_x.get()).abs() <= 1.0), "missing col-8 guide: {painted:?}");
    }

    #[test]
    fn sibling_blocks_each_get_their_own_guide() {
        // Two sibling `if`s close at the same column. Each block must draw its
        // own guide: same column but different rows — the second one is NOT on
        // top of the first, so it must not be skipped.
        let content = "fn outer() {\n    if a {\n        a();\n    }\n    if b {\n        b();\n    }\n}\n";
        let rects = guide_rects(content);
        let guides: Vec<_> = rects.iter().filter(|(x, _, _)| (*x - 40.0).abs() < 20.0).collect();
        assert_eq!(guides.len(), 2, "{guides:?}");
        assert!((guides[0].0 - guides[1].0).abs() <= 1.0, "same column expected: {guides:?}");
        assert!(guides[0].1 != guides[1].1, "guides on the same row makes no sense: {guides:?}");
    }

    #[test]
    fn active_guide_animates_for_outermost_folded_pair() {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        // Folded file: fn a's `}` is hidden, fn b's `{...}` is the outermost
        // matched pair still visible. Its ACTIVE guide must still fade in.
        let content = "fn a() {⋯\nfn b() {\n    if y {\n        z();\n    }\n}\n";
        let origin = egui::Pos2::new(4.0, 2.0);
        let overlay = EditorOverlay { bracket_guides: true, colorize_brackets: false };
        let scan = analyze_brackets(&content.chars().collect::<Vec<char>>(), &[]);
        let mut cursor_idx = 0usize; // inside the fn b body
        for p in &scan.pairs {
            if p.close != usize::MAX && p.close - p.open > content.len() / 3 {
                cursor_idx = p.open + 2;
            }
        }
        assert!(cursor_idx > 0, "no wide folded display pair found");
        for frame in 0..3u64 {
            let input = egui::RawInput {
                time: Some(frame as f64 / 60.0),
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
                ..Default::default()
            };
            let output = ctx.run_ui(input.clone(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let galley = ui.fonts_mut(|f| f.layout_job(LayoutJob::single_section(
                        content.to_string(),
                        egui::TextFormat::simple(egui::FontId::monospace(14.0), egui::Color32::WHITE),
                    )));
                    let cursor = egui::text::CCursorRange::one(egui::text::CCursor::new(cursor_idx));
                    draw_editor_overlays(ui, "anim_probe", &galley, origin, Some(cursor), &scan, &overlay, &Palette::dark(), 14.0);
                });
            });
            let mut tall = Vec::new();
            for cs in &output.shapes {
                if let egui::Shape::Rect(r) = &cs.shape {
                    if r.rect.width() <= 6.0 && r.rect.height() > 20.0 {
                        tall.push((r.rect.left().round(), r.rect.top().round(), r.rect.bottom().round(), r.fill.gamma_multiply(1.0).a()));
                    }
                }
            }
            output.drop_without_applying_deltas();
            eprintln!("frame {frame}: tall rects = {tall:?}");
        }
    }

    #[test]
    fn show_editor_paints_bracket_guides() {
        // Guards the real widget path: through the gutter + scroll areas, a
        // bracket guide per multi-line pair's close column is painted — the
        // `struct` and `impl` that close at the left margin included.
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        let content = "pub struct Button {\n    pub width: f32,\n    pub bg: Color,\n    hovered: Cell<bool>,\n}\n\nimpl Button {\n    pub fn new(label: &str, bg: Color) -> Self {\n        Button {\n            width: 120.0,\n            height: 40.0,\n        }\n    }\n}\n";
        let mut tab = crate::tabs::Tab::new("main.rs", content, Default::default());
        let overlay = EditorOverlay { bracket_guides: true, colorize_brackets: true };
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 500.0))),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                crate::editor::show_editor(ui, &mut tab, crate::theme::Theme::GithubDark, "000000", &overlay, &Palette::dark(), &mut crate::icons::Icons::new());
            });
        });
        let mut painted = Vec::new();
        for cs in &output.shapes {
            if let egui::Shape::Rect(r) = &cs.shape {
                if r.rect.width() <= 50.0 && r.rect.height() > 8.0 {
                    painted.push((r.rect.left().round(), r.rect.top().round(), r.rect.bottom().round()));
                }
            }
        }
        output.drop_without_applying_deltas();
        // `struct Button` and `impl Button` both close at the left margin: two
        // distinct col-0 guides must be painted (different start rows).
        let col0 = painted.iter().map(|(x, _, _)| *x).fold(f32::INFINITY, f32::min);
        let at_margin: Vec<_> = painted.iter().filter(|(x, _, _)| (*x - col0).abs() <= 1.0).collect();
        assert!(at_margin.len() >= 2, "struct + impl col-0 guides expected: {painted:?}");
        assert!(at_margin[0].1 != at_margin[1].1, "distinct rows expected: {painted:?}");
    }
}