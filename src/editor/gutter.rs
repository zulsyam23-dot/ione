/// Line-number gutter widget rendered to the left of the code area. Every
/// displayed row is one real line (folds collapse their interior into an
/// inline `⋯` on the opening line, they never make extra rows), so numbers
/// stay 1:1 with reality. A narrow strip on the left is reserved for the
/// chevron fold-toggle icons.

use eframe::egui::{self, TextBuffer, Ui};
use egui::text::LayoutJob;
use egui_code_editor::{ColorTheme, TokenType};

use super::{FONT_SIZE, TEXT_ROWS};
use crate::diagnostics::Severity;
use crate::editor::folds::{draw_fold_icons, FoldView};
use crate::editor::styling::format_font;
use crate::icons::Icons;
use crate::style::Palette;

/// Returns the display rows whose fold icon was clicked.
pub(crate) fn numlines_show(
    ui: &mut Ui,
    icons: &mut Icons,
    view: &FoldView,
    fold_rows: &[usize],
    id: &str,
    theme: &ColorTheme,
    diag_rows: &[(usize, Severity)],
    palette: &Palette,
) -> Vec<usize> {
    let rows = view.rows.len().max(TEXT_ROWS);
    let max_indent = rows.to_string().len();
    // Half-width columns left of the numbers, reserved for the fold icons.
    let strip = 3usize;
    let pad = 4usize + strip;
    let mut lines = Vec::with_capacity(rows);
    for (_, row) in view.rows.iter().enumerate() {
        let label = (row.line + 1).to_string();
        lines.push(format!(
            "{}{label}",
            " ".repeat(pad + max_indent.saturating_sub(label.len()))
        ));
    }
    while lines.len() < TEXT_ROWS {
        lines.push(" ".repeat(pad + max_indent));
    }
    let mut counter = lines.join("\n");

    let width = (max_indent + pad) as f32 * FONT_SIZE * 0.5;

    let mut layouter = |ui: &Ui, text_buffer: &dyn TextBuffer, _wrap_width: f32| {
        let layout_job = LayoutJob::single_section(
            text_buffer.as_str().to_string(),
            format_font(FONT_SIZE, theme.type_color(TokenType::Comment(true))),
        );
        ui.fonts_mut(|fonts| fonts.layout_job(layout_job))
    };

    let output = egui::TextEdit::multiline(&mut counter)
        .id_source(format!("{id}_numlines"))
        .font(egui::TextStyle::Monospace)
        .interactive(false)
        .frame(egui::Frame::NONE)
        .desired_rows(TEXT_ROWS)
        .desired_width(width)
        .layouter(&mut layouter)
        .show(ui);

    let mut clicked = Vec::new();
    let mut collapsed = std::collections::HashSet::new();
    for (i, r) in view.rows.iter().enumerate() {
        if r.marker.is_some() {
            collapsed.insert(i);
        }
    }
    draw_fold_icons(
        ui,
        icons,
        fold_rows,
        &collapsed,
        &output.galley,
        output.galley_pos,
        output.galley_pos.x + 2.0,
        &mut clicked,
    );

    // Diagnostics markers: a short colored bar over the line number, flush
    // against the editor edge. Error (red) wins over warning when both exist.
    if !diag_rows.is_empty() {
        let marker_w = 3.0_f32;
        let marker_h = 12.0_f32;
        for &(row, sev) in diag_rows {
            let Some(grow) = output.galley.rows.get(row) else {
                continue;
            };
            let y = output.galley_pos.y + grow.pos.y + (grow.size.y - marker_h) / 2.0;
            let x = output.galley_pos.x + 2.0; // into the icon strip, left of the numbers
            let rect = egui::Rect::from_min_size(
                egui::Pos2::new(x, y),
                egui::vec2(marker_w, marker_h),
            );
            let color = match sev {
                Severity::Error => palette.diag_error,
                Severity::Warning => palette.diag_warning,
            };
            ui.painter().rect_filled(rect, 0.0, color);
        }
    }

    clicked
}