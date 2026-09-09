use eframe::egui;

use crate::app::AppCommand;
use crate::fonts;
use crate::guides::EditorOverlay;

pub fn show_menu_bar(
    ui: &mut egui::Ui,
    commands: &mut Vec<AppCommand>,
    current_font: &str,
    overlay: &EditorOverlay,
    recent: &[std::path::PathBuf],
) {
    egui::menu::MenuBar::new().ui(ui, |ui| {
        egui::menu::MenuButton::new("File").ui(ui, |ui| {
            if ui
                .add(egui::Button::new("New File").shortcut_text("Ctrl+N"))
                .clicked()
            {
                commands.push(AppCommand::NewFile);
                ui.close();
            }
            if ui
                .add(egui::Button::new("Open...").shortcut_text("Ctrl+O"))
                .clicked()
            {
                commands.push(AppCommand::OpenFile);
                ui.close();
            }
            if ui
                .add(egui::Button::new("Open Folder...").shortcut_text("Ctrl+K Ctrl+O"))
                .clicked()
            {
                commands.push(AppCommand::OpenFolder);
                ui.close();
            }
            ui.separator();
            if ui
                .add(egui::Button::new("Quick Open").shortcut_text("Ctrl+P"))
                .clicked()
            {
                commands.push(AppCommand::QuickOpen);
                ui.close();
            }
            ui.menu_button("Open Recent", |ui| {
                if recent.is_empty() {
                    ui.add_enabled(false, egui::Button::new("No recent files"));
                }
                for p in recent {
                    let label = p
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if ui
                        .button(label)
                        .on_hover_text(p.to_string_lossy().to_string())
                        .clicked()
                    {
                        commands.push(AppCommand::OpenRecent(p.clone()));
                        ui.close();
                    }
                }
            });
            ui.separator();
            if ui
                .add(egui::Button::new("Save").shortcut_text("Ctrl+S"))
                .clicked()
            {
                commands.push(AppCommand::Save);
                ui.close();
            }
            if ui
                .add(egui::Button::new("Save As...").shortcut_text("Ctrl+Shift+S"))
                .clicked()
            {
                commands.push(AppCommand::SaveAs);
                ui.close();
            }
            ui.separator();
            if ui
                .add(egui::Button::new("Close Tab").shortcut_text("Ctrl+W"))
                .clicked()
            {
                commands.push(AppCommand::CloseTab);
                ui.close();
            }
            ui.separator();
            if ui.add(egui::Button::new("Exit")).clicked() {
                commands.push(AppCommand::Exit);
                ui.close();
            }
        });

        egui::menu::MenuButton::new("Edit").ui(ui, |ui| {
            if ui
                .add(egui::Button::new("Find & Replace").shortcut_text("Ctrl+H"))
                .clicked()
            {
                commands.push(AppCommand::ToggleSearch);
                ui.close();
            }
            ui.separator();
            if ui
                .add(egui::Button::new("Fold Block").shortcut_text("Ctrl+Shift+["))
                .clicked()
            {
                commands.push(AppCommand::Fold);
                ui.close();
            }
            if ui
                .add(egui::Button::new("Unfold Block").shortcut_text("Ctrl+Shift+]"))
                .clicked()
            {
                commands.push(AppCommand::Unfold);
                ui.close();
            }
        });

        egui::menu::MenuButton::new("View").ui(ui, |ui| {
            if ui
                .add(egui::Button::new("Toggle File Explorer").shortcut_text("Ctrl+L"))
                .clicked()
            {
                commands.push(AppCommand::ToggleSidebar);
                ui.close();
            }
            ui.separator();
            if ui
                .add(egui::Button::new("Toggle Terminal").shortcut_text("Ctrl+`"))
                .clicked()
            {
                commands.push(AppCommand::ToggleTerminal);
                ui.close();
            }
            ui.separator();
            ui.menu_button("Editor Guides", |ui| {
                if ui
                    .button(checked("Bracket Pair Guides", overlay.bracket_guides))
                    .clicked()
                {
                    commands.push(AppCommand::ToggleBracketGuides);
                    ui.close();
                }
                if ui
                    .button(checked("Rainbow Brackets", overlay.colorize_brackets))
                    .clicked()
                {
                    commands.push(AppCommand::ToggleBracketColorize);
                    ui.close();
                }
            });
            ui.menu_button("Theme", |ui| {
                for theme in crate::theme::Theme::ALL {
                    let label = if theme.is_beta() {
                        format!("{} (beta)", theme.name())
                    } else {
                        theme.name().to_string()
                    };
                    if ui.button(label).clicked() {
                        commands.push(AppCommand::SetTheme(*theme));
                        ui.close();
                    }
                }
            });
            ui.menu_button("Editor Font", |ui| {
                for &(name, _) in fonts::FONTS {
                    let selected = name == current_font;
                    let label = if selected {
                        format!("✓ {name}")
                    } else {
                        name.to_string()
                    };
                    if ui.button(label).clicked() {
                        commands.push(AppCommand::SetEditorFont(name.to_string()));
                        ui.close();
                    }
                }
            });
        });

        egui::menu::MenuButton::new("Help").ui(ui, |ui| {
            if ui.button("About").clicked() {
                commands.push(AppCommand::About);
                ui.close();
            }
        });
    });
}

fn checked(label: &str, on: bool) -> String {
    if on {
        format!("✓ {label}")
    } else {
        label.to_string()
    }
}

pub fn handle_shortcuts(ctx: &egui::Context, commands: &mut Vec<AppCommand>, ctrl_k_pending: &mut bool) {
    ctx.input(|i| {
        let ctrl = i.modifiers.ctrl && !i.modifiers.alt;
        let chord = *ctrl_k_pending;

        // Count "normal" key presses to reset a half-typed chord.
        let mut any_key = false;
        for e in &i.events {
            if let egui::Event::Key { pressed: true, .. } = e {
                any_key = true;
            }
        }

        if ctrl && i.key_pressed(egui::Key::K) {
            if !i.modifiers.shift {
                *ctrl_k_pending = true;
            }
        } else if chord && ctrl && i.key_pressed(egui::Key::O) && !i.modifiers.shift {
            commands.push(AppCommand::OpenFolder);
            *ctrl_k_pending = false;
        } else if chord && any_key {
            // Some other key ended the chord without completing it.
            *ctrl_k_pending = false;
        }

        if ctrl && !i.modifiers.shift && !chord {
            if i.key_pressed(egui::Key::N) {
                commands.push(AppCommand::NewFile);
            }
            if i.key_pressed(egui::Key::O) {
                commands.push(AppCommand::OpenFile);
            }
            if i.key_pressed(egui::Key::P) {
                commands.push(AppCommand::QuickOpen);
            }
            if i.key_pressed(egui::Key::S) {
                commands.push(AppCommand::Save);
            }
            if i.key_pressed(egui::Key::W) {
                commands.push(AppCommand::CloseTab);
            }
            if i.key_pressed(egui::Key::H) {
                commands.push(AppCommand::ToggleSearch);
            }
            if i.key_pressed(egui::Key::D) {
                commands.push(AppCommand::MultiSelectNext);
            }
            if i.key_pressed(egui::Key::L) {
                commands.push(AppCommand::ToggleSidebar);
            }
        }
        if ctrl && i.modifiers.shift && !chord {
            if i.key_pressed(egui::Key::S) {
                commands.push(AppCommand::SaveAs);
            }
            if i.key_pressed(egui::Key::OpenBracket) {
                commands.push(AppCommand::Fold);
            }
            if i.key_pressed(egui::Key::CloseBracket) {
                commands.push(AppCommand::Unfold);
            }
        }
        if ctrl && i.key_pressed(egui::Key::Tab) && !chord {
            if i.modifiers.shift {
                commands.push(AppCommand::PrevTab);
            } else {
                commands.push(AppCommand::NextTab);
            }
        }
        if ctrl && !chord {
            for e in &i.events {
                if let egui::Event::Text(t) = e {
                    if t == "`" {
                        commands.push(AppCommand::ToggleTerminal);
                    }
                }
            }
        }
    });
}
