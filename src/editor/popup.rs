//! The auto-complete suggestion popup painted under the editor caret.

use eframe::egui;

use egui_code_editor::{ColorTheme, TokenType};

use crate::completion::{self, CompletionState};
use crate::style::Palette;

use super::FONT_SIZE;

/// Draw the suggestion popup under the caret (`anchor` = caret glyph origin),
/// flipping above it when it would fall out of the visible viewport. Painted
/// through the same scrolled painter as the guides, so it tracks the text.
/// Pure shapes, no widget: clicks pass through to TextEdit (which closes the
/// popup), navigation is keyboard-only.
pub(crate) fn draw_completion_popup(
    ui: &egui::Ui,
    anchor: egui::Pos2,
    st: &CompletionState,
    palette: &Palette,
    theme: &ColorTheme,
) {
    let n = st.items.len();
    if n == 0 {
        return;
    }
    let font = egui::FontId::monospace(FONT_SIZE);
    let row_h = FONT_SIZE + 6.0;
    let pad = 6.0;
    let txt_color = |k: completion::Kind| match k {
        completion::Kind::Keyword => theme.type_color(TokenType::Keyword),
        completion::Kind::Type => theme.type_color(TokenType::Type),
        completion::Kind::Special => theme.type_color(TokenType::Special),
        completion::Kind::Func => theme.type_color(TokenType::Function),
        completion::Kind::Word => palette.text,
    };
    let mut widths: Vec<f32> = Vec::with_capacity(n);
    let mut max_text = 40.0f32;
    for it in &st.items {
        let tw = ui.fonts_mut(|f| {
            f.layout_no_wrap(it.text.clone(), font.clone(), egui::Color32::WHITE)
                .size()
                .x
        });
        let dw = if it.detail.is_empty() {
            0.0
        } else {
            ui.fonts_mut(|f| {
                f.layout_no_wrap(it.detail.clone(), font.clone(), egui::Color32::WHITE)
                    .size()
                    .x
            }) + 10.0
        };
        max_text = max_text.max(tw + dw);
        widths.push(tw);
    }
    let mut pop = egui::Rect::from_min_size(
        anchor + egui::vec2(0.0, FONT_SIZE + 2.0),
        egui::vec2(max_text + pad * 2.0, n as f32 * row_h + 4.0),
    );
    let clip = ui.clip_rect();
    if pop.bottom() > clip.bottom() {
        pop = pop.translate(egui::vec2(0.0, -(FONT_SIZE + 2.0) - row_h - pop.height()));
    }
    ui.painter().rect(
        pop,
        3.0,
        palette.panel,
        egui::Stroke::new(1.0, palette.border),
        egui::StrokeKind::Inside,
    );
    for (i, (it, _w)) in st.items.iter().zip(widths).enumerate() {
        let row = egui::Rect::from_min_size(
            egui::Pos2::new(pop.left(), pop.top() + 2.0 + i as f32 * row_h),
            egui::vec2(pop.width(), row_h),
        );
        if i == st.selected {
            ui.painter()
                .rect_filled(row.shrink(1.0), 2.0, palette.accent.gamma_multiply(0.25));
        }
        ui.painter().text(
            row.min + egui::vec2(pad, 1.0),
            egui::Align2::LEFT_TOP,
            it.text.clone(),
            font.clone(),
            txt_color(it.kind),
        );
        if !it.detail.is_empty() {
            ui.painter().text(
                egui::Pos2::new(pop.right() - pad, row.top() + 1.0),
                egui::Align2::RIGHT_TOP,
                it.detail.clone(),
                font.clone(),
                palette.text_muted,
            );
        }
    }
}