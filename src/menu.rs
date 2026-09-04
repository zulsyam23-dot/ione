use eframe::egui;

use crate::app::AppCommand;
use crate::fonts;

pub fn show_menu_bar(ui: &mut egui::Ui, commands: &mut Vec<AppCommand>, current_font: &str) {
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
            ui.menu_button("Theme", |ui| {
                for theme in crate::theme::Theme::ALL {
                    if ui.button(theme.name()).clicked() {
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
            if i.key_pressed(egui::Key::S) {
                commands.push(AppCommand::Save);
            }
            if i.key_pressed(egui::Key::W) {
                commands.push(AppCommand::CloseTab);
            }
            if i.key_pressed(egui::Key::H) {
                commands.push(AppCommand::ToggleSearch);
            }
            if i.key_pressed(egui::Key::L) {
                commands.push(AppCommand::ToggleSidebar);
            }
        }
        if ctrl && i.modifiers.shift && !chord {
            if i.key_pressed(egui::Key::S) {
                commands.push(AppCommand::SaveAs);
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
