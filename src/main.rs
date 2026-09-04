mod app;
mod editor;
mod icons;
mod tabs;
mod menu;
mod file_tree;
mod outline;
mod search;
mod style;
mod terminal;
mod theme;
mod loading;
mod fonts;

use eframe::egui;
use style::{apply_style, Palette};

fn main() -> eframe::Result<()> {
    let icon = load_icon();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([600.0, 400.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "ione",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            setup_visuals(&cc.egui_ctx);
            Ok(Box::new(app::EditorApp::new()))
        }),
    )
}

fn load_icon() -> egui::IconData {
    let icon_bytes = include_bytes!("..\\assets\\icons\\1770523897143.ico");
    let image = image::load_from_memory(icon_bytes)
        .expect("Failed to load icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

fn setup_visuals(ctx: &egui::Context) {
    apply_style(ctx, Palette::dark());
}
