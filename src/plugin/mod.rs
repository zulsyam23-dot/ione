//! Plugin API: hooks that let external crates re-skin and extend ione.
//!
//! A plugin is a value implementing [`Plugin`]. Plugins are registered on the
//! [`PluginRegistry`] (owned by `EditorApp`), which drives their lifecycle:
//! `on_load` once at startup, `on_tick` every frame with a [`PluginContext`]
//! exposing the live UI state they are allowed to adjust.

use crate::core::style::Palette;

/// Live UI state a plugin may read or tweak each frame.
pub struct PluginContext {
    pub palette: Palette,
    pub bracket_guides: bool,
    pub colorize_brackets: bool,
}

pub trait Plugin {
    fn name(&self) -> &str;

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    /// Called once when the plugin is registered/loaded.
    fn on_load(&mut self, _ctx: &mut PluginContext) {}

    /// Called every frame before the UI is painted.
    fn on_tick(&mut self, _ctx: &mut PluginContext) {}

    /// Called every frame so plugins can draw their own windows/panels.
    fn on_ui(&mut self, _egui_ctx: &eframe::egui::Context, _ctx: &mut PluginContext) {}

    /// Ask the plugin to show/hide its window, if it has one.
    fn toggle_window(&mut self) {}

    /// Whether this plugin wants its docked right-side panel visible.
    fn dock_open(&self) -> bool {
        false
    }

    /// Draw the plugin's docked panel contents.
    fn on_dock(&mut self, _ui: &mut eframe::egui::Ui, _ctx: &mut PluginContext) {}
}

pub mod builtin;

#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, mut plugin: Box<dyn Plugin>, ctx: &mut PluginContext) {
        plugin.on_load(ctx);
        self.plugins.push(plugin);
    }

    pub fn on_tick(&mut self, ctx: &mut PluginContext) {
        for p in &mut self.plugins {
            p.on_tick(ctx);
        }
    }

    pub fn on_ui(&mut self, egui_ctx: &eframe::egui::Context, ctx: &mut PluginContext) {
        for p in &mut self.plugins {
            p.on_ui(egui_ctx, ctx);
        }
    }

    pub fn toggle_window(&mut self, name: &str) {
        for p in &mut self.plugins {
            if p.name() == name {
                p.toggle_window();
            }
        }
    }

    pub fn any_dock_open(&self) -> bool {
        self.plugins.iter().any(|p| p.dock_open())
    }

    pub fn on_dock(&mut self, ui: &mut eframe::egui::Ui, ctx: &mut PluginContext) {
        for p in &mut self.plugins {
            if p.dock_open() {
                p.on_dock(ui, ctx);
            }
        }
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.plugins.iter().map(|p| p.name())
    }
}
