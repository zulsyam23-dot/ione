use std::path::PathBuf;

use eframe::egui::{self, Align, Color32, Layout, RichText};

use crate::diagnostics::Severity;
use crate::icons::Icon;
use crate::menu;
use crate::style::{frame, header_label, Palette};
use crate::tabs::TabManager;

use super::AppCommand;
use super::EditorApp;

impl EditorApp {
    pub(super) fn show_title_bar(&mut self, root_ui: &mut egui::Ui, commands: &mut Vec<AppCommand>) {
        egui::Panel::top("title_bar")
            .frame(frame(self.palette.bg, Color32::TRANSPARENT, 0, 3))
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    egui::menu::MenuBar::new().ui(ui, |ui| {
                        menu::show_menu_bar(ui, commands, &self.editor_font, &self.guides(), &self.recent_files);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(8.0);
                        if self
                            .icons
                            .image_button(ui, Icon::Terminal, 14.0, "Toggle Terminal (Ctrl+`)")
                            .clicked()
                        {
                            commands.push(AppCommand::ToggleTerminal);
                        }
                    });
                });
            });
    }

    pub(super) fn show_status_bar(root_ui: &mut egui::Ui, tabs: &TabManager, branch: &str) {
        let p = Palette::dark();
        egui::Panel::bottom("status_bar")
            .frame(frame(p.bg, p.border, 0, 5))
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(header_label(ui, &format!(" {}", branch)));
                    ui.separator();
                    if let Some(tab) = tabs.active_tab() {
                        let lines = tab.content.chars().filter(|&c| c == '\n').count() + 1;
                        ui.label(
                            RichText::new(format!("Ln {}, Col {}  ({lines} lines)", tab.cursor_line + 1, tab.cursor_col + 1))
                                .color(p.text),
                        );
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
                        ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).color(p.text_muted));
                    });
                });
            });
    }

    pub(super) fn show_tab_bar(tabs: &mut TabManager, ui: &mut egui::Ui, p: Palette) {
        if tabs.is_empty() {
            return;
        }

        let mut switch_to: Option<usize> = None;
        let mut close_idx: Option<usize> = None;

        egui::Panel::top("tab_bar")
            .frame(frame(ui.visuals().extreme_bg_color, Color32::TRANSPARENT, 0, 5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    for (i, tab) in tabs.tabs.iter().enumerate() {
                        let active = i == tabs.active;
                        let text = tab.display_name();
                        let label = if tab.dirty {
                            format!("● {text}")
                        } else {
                            text
                        };
                        let color = if active { p.text } else { p.text_muted };
                        let fill = if active { p.panel_active } else { Color32::TRANSPARENT };
                        let tab_frame = egui::Frame::NONE
                            .fill(fill)
                            .corner_radius(0.0);
                        let response = tab_frame.show(ui, |ui| {
                            ui.add(
                                egui::Button::new(RichText::new(&label).color(color)),
                            )
                        })
                        .inner;

                        if response.clicked() {
                            switch_to = Some(i);
                        }
                        response.context_menu(|ui| {
                            if ui.button("Close").clicked() {
                                close_idx = Some(i);
                                ui.close();
                            }
                        });
                        if response.middle_clicked() {
                            close_idx = Some(i);
                        }
                        ui.add_space(2.0);
                    }
                });
            });

        if let Some(idx) = switch_to {
            tabs.set_active(idx);
        }
        if let Some(idx) = close_idx {
            tabs.close(idx);
        }
    }

    pub(super) fn show_breadcrumbs(&self, ui: &mut egui::Ui, commands: &mut Vec<AppCommand>) {
        let Some(tab) = self.tabs.active_tab() else {
            return;
        };
        let Some(path) = &tab.path else {
            return;
        };
        egui::Panel::top("breadcrumbs")
            .frame(frame(self.palette.panel, Color32::TRANSPARENT, 0, 8))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut cumulative = PathBuf::new();
                    for comp in path.components() {
                        use std::path::Component;
                        match comp {
                            Component::RootDir => {
                                cumulative.push(std::path::MAIN_SEPARATOR.to_string());
                            }
                            Component::CurDir | Component::ParentDir => continue,
                            Component::Normal(os) => {
                                cumulative.push(os);
                                let seg = cumulative.clone();
                                let label = os.to_string_lossy().to_string();
                                let is_file = cumulative == *path;
                                let color = if is_file {
                                    self.palette.text_muted
                                } else {
                                    self.palette.text
                                };
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new(&label).color(color))
                                            .frame(false),
                                    )
                                    .on_hover_text(seg.to_string_lossy().to_string())
                                    .clicked()
                                {
                                    commands.push(AppCommand::SetRoot(seg));
                                }
                                ui.label(RichText::new("/").color(self.palette.text_muted));
                            }
                            Component::Prefix(_) => {}
                        }
                    }
                });
            });
    }
}