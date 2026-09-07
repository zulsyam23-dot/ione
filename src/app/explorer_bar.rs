use eframe::egui::{self, Align, Align2, Layout};

use crate::icons::Icon;
use crate::style::header_label;

use super::utils::{drag_vertical_splitter, load_logo, HANDLE};
use super::AppCommand;
use super::EditorApp;

impl EditorApp {
    pub(super) fn show_sidebar(&mut self, commands: &mut Vec<AppCommand>, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(header_label(ui, "Explorer"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self
                    .icons
                    .image_button(ui, Icon::Refresh, 14.0, "Refresh")
                    .clicked()
                {
                    commands.push(AppCommand::RefreshFileTree);
                }
                if self
                    .icons
                    .image_button(ui, Icon::FilePlus, 14.0, "New File")
                    .clicked()
                {
                    commands.push(AppCommand::NewFile);
                }
                if self
                    .icons
                    .image_button(ui, Icon::FolderPlus, 14.0, "New Folder")
                    .clicked()
                {
                    commands.push(AppCommand::NewFolder(None));
                }
            });
        });
        ui.add_space(8.0);

        let total_h = ui.available_height();
        let total_w = ui.available_width();
        let tree_h = ((total_h - HANDLE) * (1.0 - self.outline_frac)).max(40.0);
        let outline_h = (total_h - tree_h - HANDLE).max(40.0);

        ui.allocate_ui(egui::vec2(total_w, tree_h), |ui| {
            self.file_tree.show(ui, &mut self.icons, &self.palette, commands);
        });

        drag_vertical_splitter(ui, self.palette.border, &mut self.outline_frac, total_h);

        ui.allocate_ui(egui::vec2(total_w, outline_h), |ui| {
            // Symbols come from the tab cache (recomputed only on content
            // change), never re-extracted per frame.
            let symbols = match self.tabs.active_tab() {
                Some(tab) => Some(&tab.cache.symbols[..]),
                None => None,
            };
            if let Some(symbols) = symbols {
                if !symbols.is_empty() {
                    if let Some(line) = self.outline.show(ui, &self.palette, symbols) {
                        if let Some(tab) = self.tabs.active_tab_mut() {
                            tab.goto_line = Some(line);
                        }
                    }
                }
            }
        });
    }

    pub(super) fn show_empty_state(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Center the block strictly within the area that remains after the
        // terminal/status panels, so it can never spill over them.
        let avail = ui.available_rect_before_wrap();

        if self.logo.is_none() {
            self.logo = Some(load_logo(ctx));
        }

        let logo_sz = 72.0_f32;
        let gap = 16.0_f32;
        let title_h = 40.0_f32;
        let sub_h = 24.0_f32;
        let total = logo_sz + gap + title_h + 10.0 + sub_h;
        let cx = avail.center().x;
        let mut y = avail.center().y - total / 2.0;

        if let Some(tex) = &self.logo {
            let rect = egui::Rect::from_center_size(
                egui::pos2(cx, y + logo_sz / 2.0),
                egui::vec2(logo_sz, logo_sz),
            );
            ui.painter().image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        y += logo_sz + gap;

        let title_rc = egui::Rect::from_center_size(
            egui::pos2(cx, y + title_h / 2.0),
            egui::vec2(300.0, title_h),
        );
        ui.painter().text(
            title_rc.center(),
            Align2::CENTER_CENTER,
            "ione",
            egui::FontId::proportional(36.0),
            self.palette.text,
        );
        y += title_h + 10.0;

        let sub_rc = egui::Rect::from_center_size(
            egui::pos2(cx, y + sub_h / 2.0),
            egui::vec2(500.0, sub_h),
        );
        ui.painter().text(
            sub_rc.center(),
            Align2::CENTER_CENTER,
            "Open a file or create a new one to get started.",
            egui::FontId::proportional(15.0),
            self.palette.text_muted,
        );
    }
}