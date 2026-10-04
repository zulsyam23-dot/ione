use eframe::egui::{self, Align, Layout, RichText};

use crate::app::{AppCommand, EditorApp};
use crate::core::style::{Palette, frame, header_label};
use crate::editor::diagnostics::Severity;
use crate::git::GitPanel;
use crate::workspace::tabs::TabManager;

pub(super) fn show(root_ui: &mut egui::Ui, app: &EditorApp, commands: &mut Vec<AppCommand>) {
    show_status_bar(root_ui, &app.tabs, &app.branch(), &app.git, commands);
}

fn show_status_bar(
    root_ui: &mut egui::Ui,
    tabs: &TabManager,
    branch: &str,
    git: &GitPanel,
    commands: &mut Vec<AppCommand>,
) {
    let p = Palette::dark();
    egui::Panel::bottom("status_bar")
        .frame(frame(p.bg, p.border, 0, 5))
        .show(root_ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(header_label(ui, &format!(" {}", branch)));
                ui.separator();
                if let Some(tab) = tabs.active_tab() {
                    let lines = tab.content.chars().filter(|&c| c == '\n').count() + 1;
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!(
                                    "Ln {}, Col {}  ({lines} lines)",
                                    tab.cursor_line + 1,
                                    tab.cursor_col + 1
                                ))
                                .color(p.text),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Go to Line (Ctrl+G)")
                        .clicked()
                    {
                        commands.push(AppCommand::GoToLine);
                    }
                    ui.separator();
                    ui.label(RichText::new("UTF-8").color(p.text_muted));
                    let errs = tab
                        .cache
                        .diagnostics
                        .iter()
                        .filter(|d| d.severity == Severity::Error)
                        .count();
                    let warns = tab
                        .cache
                        .diagnostics
                        .iter()
                        .filter(|d| d.severity == Severity::Warning)
                        .count();
                    if errs > 0 || warns > 0 {
                        ui.separator();
                        ui.label(
                            RichText::new(format!("{errs} errors · {warns} warnings"))
                                .color(p.text),
                        );
                    }
                } else {
                    ui.label(RichText::new("No file open").color(p.text_muted));
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(tab) = tabs.active_tab() {
                        let lang = if tab.path.is_none() {
                            "Plain Text".to_string()
                        } else {
                            tab.syntax.language().to_string()
                        };
                        ui.label(RichText::new(lang).color(p.text));
                        ui.separator();
                    }
                    ui.separator();
                    if let Some(g) = git.status_bar_suffix() {
                        ui.label(RichText::new(g).color(p.text_muted));
                        ui.separator();
                    }
                    ui.label(
                        RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                            .color(p.text_muted),
                    );
                });
            });
        });
}
