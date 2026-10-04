use eframe::egui::{self, Align, Layout};

use crate::app::{AppCommand, EditorApp};
use crate::core::style::{frame, header_label};

pub(super) fn show(
    app: &mut EditorApp,
    root_ui: &mut egui::Ui,
    ctx: &egui::Context,
    commands: &mut Vec<AppCommand>,
) -> Option<egui::Rect> {
    app.terminal.cwd = app.file_tree.root.clone();
    if !app.terminal.visible {
        return None;
    }

    Some(
        egui::Panel::bottom("terminal_panel")
            .resizable(true)
            .default_size(220.0)
            .min_size(80.0)
            .frame(frame(app.palette.panel, app.palette.border, 0, 0))
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(header_label(ui, "Terminal"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(8.0);
                        if ui.small_button("×").on_hover_text("Close").clicked() {
                            commands.push(AppCommand::ToggleTerminal);
                        }
                        if ui
                            .small_button("Copy")
                            .on_hover_text("Copy selection")
                            .clicked()
                            && let Some(text) = app.terminal.copy_selection()
                        {
                            ctx.copy_text(text);
                        }
                    });
                });
                app.terminal.show(ui, &app.palette, ctx);
            })
            .response
            .rect,
    )
}
