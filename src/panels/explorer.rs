use eframe::egui::{self, Align, Align2, Layout};

use crate::app::utils::{HANDLE, drag_vertical_splitter, load_logo};
use crate::app::{AppCommand, EditorApp};
use crate::core::icons::Icon;
use crate::core::style::header_label;

pub(super) fn show(
    app: &mut EditorApp,
    root_ui: &mut egui::Ui,
    commands: &mut Vec<AppCommand>,
) -> Option<egui::Rect> {
    if !app.panels.explorer_visible {
        return None;
    }

    Some(
        egui::Panel::left("sidebar")
            .exact_size(230.0)
            .show_separator_line(false)
            .frame(crate::core::style::frame(
                app.palette.panel,
                app.palette.border,
                0,
                8,
            ))
            .show(root_ui, |ui| show_contents(app, commands, ui))
            .response
            .rect,
    )
}

fn show_contents(app: &mut EditorApp, commands: &mut Vec<AppCommand>, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(header_label(ui, "Explorer"));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if app
                .icons
                .image_button(ui, Icon::Refresh, 16.0, "Refresh")
                .clicked()
            {
                commands.push(AppCommand::RefreshFileTree);
            }
            if app
                .icons
                .image_button(ui, Icon::FilePlus, 16.0, "New File")
                .clicked()
            {
                commands.push(AppCommand::NewFile);
            }
            if app
                .icons
                .image_button(ui, Icon::FolderPlus, 16.0, "New Folder")
                .clicked()
            {
                commands.push(AppCommand::NewFolder(None));
            }
        });
    });
    ui.add_space(8.0);

    let total_h = ui.available_height();
    let total_w = ui.available_width();
    let tree_h = ((total_h - HANDLE) * (1.0 - app.panels.outline_fraction)).max(40.0);
    let outline_h = (total_h - tree_h - HANDLE).max(40.0);

    ui.allocate_ui(egui::vec2(total_w, tree_h), |ui| {
        let tints = match app.file_tree.root.as_ref() {
            Some(root) => app.git.explorer_tints(root, &app.palette),
            None => std::collections::HashMap::new(),
        };
        app.file_tree
            .show(ui, &mut app.icons, &app.palette, commands, Some(&tints));
    });

    drag_vertical_splitter(
        ui,
        app.palette.border,
        &mut app.panels.outline_fraction,
        total_h,
    );

    ui.allocate_ui(egui::vec2(total_w, outline_h), |ui| {
        let symbols = app.tabs.active_tab().map(|tab| &tab.cache.symbols[..]);
        if let Some(symbols) = symbols
            && !symbols.is_empty()
            && let Some(line) = app.outline.show(ui, &app.palette, symbols)
            && let Some(tab) = app.tabs.active_tab_mut()
        {
            tab.goto_line = Some(line);
        }
    });
}

pub(crate) fn show_empty_state(app: &mut EditorApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let avail = ui.available_rect_before_wrap();

    if app.logo.is_none() {
        app.logo = Some(load_logo(ctx));
    }

    let logo_sz = 72.0_f32;
    let gap = 16.0_f32;
    let title_h = 40.0_f32;
    let sub_h = 24.0_f32;
    let total = logo_sz + gap + title_h + 10.0 + sub_h;
    let cx = avail.center().x;
    let mut y = avail.center().y - total / 2.0;

    if let Some(tex) = &app.logo {
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
        app.palette.text,
    );
    y += title_h + 10.0;

    let sub_rc =
        egui::Rect::from_center_size(egui::pos2(cx, y + sub_h / 2.0), egui::vec2(500.0, sub_h));
    ui.painter().text(
        sub_rc.center(),
        Align2::CENTER_CENTER,
        "Open a file or create a new one to get started.",
        egui::FontId::proportional(15.0),
        app.palette.text_muted,
    );
}
