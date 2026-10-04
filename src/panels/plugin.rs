use eframe::egui::{self, Align, Layout};

use crate::app::{AppCommand, EditorApp};
use crate::core::icons::Icon;
use crate::core::style::{frame, header_label};

pub(super) fn show(
    app: &mut EditorApp,
    root_ui: &mut egui::Ui,
    commands: &mut Vec<AppCommand>,
    terminal_rect: Option<egui::Rect>,
) {
    if !app.plugins.any_dock_open() {
        return;
    }

    let rect = egui::Panel::right("plugin_dock")
        .exact_size(420.0)
        .show_separator_line(false)
        .frame(frame(app.palette.panel, app.palette.border, 0, 8))
        .show(root_ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(header_label(ui, "AI Chat"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(4.0);
                    if app
                        .icons
                        .image_button(ui, Icon::Close, 16.0, "Close")
                        .clicked()
                    {
                        commands.push(AppCommand::ToggleAiChat);
                    }
                });
            });
            ui.add_space(6.0);
            let mut pctx = app.plugin_context();
            app.plugins.on_dock(ui, &mut pctx);
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
