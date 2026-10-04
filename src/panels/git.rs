use eframe::egui;

use crate::app::{AppCommand, EditorApp};
use crate::core::style::frame;

pub(super) fn show(
    app: &mut EditorApp,
    root_ui: &mut egui::Ui,
    commands: &mut Vec<AppCommand>,
    terminal_rect: Option<egui::Rect>,
) {
    if !app.git.visible {
        return;
    }

    let rect = egui::Panel::right("git_panel")
        .exact_size(280.0)
        .show_separator_line(false)
        .frame(frame(app.palette.panel, app.palette.border, 0, 8))
        .show(root_ui, |ui| {
            app.git.show(
                ui,
                &mut app.icons,
                &app.palette,
                app.file_tree.root.as_ref(),
                commands,
            );
        })
        .response
        .rect;
    if let Some(divider_rect) = super::divider_rect(rect, terminal_rect) {
        root_ui.painter().vline(
            divider_rect.left() + 0.5,
            divider_rect.y_range(),
            egui::Stroke::new(1.0, app.palette.border),
        );
    }
}
