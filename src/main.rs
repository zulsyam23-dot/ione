mod app;
mod editor;
mod icons;
mod tabs;
mod menu;
mod guides;
mod file_tree;
mod outline;
mod search;
mod style;
mod terminal;
mod theme;
mod loading;
mod fonts;
mod diagnostics;

use eframe::egui;
use style::{apply_style, Palette};

fn main() -> eframe::Result<()> {
    install_panic_hook();
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
    let icon_bytes = include_bytes!("..\\assets\\icons\\app\\1770523897143.ico");
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

/// Log any panic (with full backtrace) to `crash.log` instead of silently
/// dying in a console window that nobody reads.
fn install_panic_hook() {
    use std::io::Write;
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let msg = format!("panic: {info}\n\nBacktrace:\n{bt}\n");
        // Next to the executable, not the (unpredictable) working directory.
        let log_path = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("crash.log")))
            .unwrap_or_else(|| std::path::PathBuf::from("crash.log"));
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let _ = f.write_all(msg.as_bytes());
        }
        eprintln!("{msg}");
    }));
}
