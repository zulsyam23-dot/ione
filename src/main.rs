mod app;
mod completion;
mod diagnostics;
mod editor;
mod file_tree;
mod fonts;
mod guides;
mod icons;
mod loading;
mod menu;
mod outline;
mod search;
mod settings;
mod style;
mod tabs;
mod terminal;
mod theme;

use eframe::egui;
use style::{Palette, apply_style};

fn main() -> eframe::Result<()> {
    install_panic_hook();
    let viewport = {
        let builder = egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([600.0, 400.0]);
        if let Some(icon) = load_icon() {
            builder.with_icon(icon)
        } else {
            builder
        }
    };
    let options = eframe::NativeOptions {
        viewport,
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

fn load_icon() -> Option<egui::IconData> {
    let icon_bytes = include_bytes!("..\\assets\\icons\\app\\1770523897143.ico");
    let image = image::load_from_memory(icon_bytes).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
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
