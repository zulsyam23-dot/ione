//! Central composition and visibility controls for the app's docked panels.

mod explorer;
mod git;
mod plugin;
mod status_bar;
mod terminal;

use eframe::egui;

use crate::app::{AppCommand, EditorApp};

#[derive(Debug, Clone, Copy)]
pub enum Panel {
    Explorer,
    Terminal,
    SourceControl,
    AiChat,
}

pub struct PanelManager {
    pub explorer_visible: bool,
    pub outline_fraction: f32,
}

impl Default for PanelManager {
    fn default() -> Self {
        Self {
            explorer_visible: true,
            outline_fraction: 0.4,
        }
    }
}

impl PanelManager {
    pub fn show_empty_state(app: &mut EditorApp, ui: &mut egui::Ui, ctx: &egui::Context) {
        explorer::show_empty_state(app, ui, ctx);
    }

    pub fn toggle(app: &mut EditorApp, panel: Panel, ctx: &egui::Context) {
        match panel {
            Panel::Explorer => app.panels.explorer_visible = !app.panels.explorer_visible,
            Panel::Terminal => app.terminal.toggle(ctx),
            Panel::SourceControl => app.git.toggle(),
            Panel::AiChat => app.plugins.toggle_window("ai-chat"),
        }
    }

    pub fn show_all(
        app: &mut EditorApp,
        root_ui: &mut egui::Ui,
        ctx: &egui::Context,
        commands: &mut Vec<AppCommand>,
    ) -> Option<egui::Rect> {
        let sidebar_rect = explorer::show(app, root_ui, commands);
        status_bar::show(root_ui, app, commands);
        let terminal_rect = terminal::show(app, root_ui, ctx, commands);
        git::show(app, root_ui, commands, terminal_rect);
        plugin::show(app, root_ui, commands, terminal_rect);
        sidebar_rect
    }
}

fn divider_rect(panel_rect: egui::Rect, terminal_rect: Option<egui::Rect>) -> Option<egui::Rect> {
    let bottom = terminal_rect
        .map(|terminal| panel_rect.bottom().min(terminal.top()))
        .unwrap_or_else(|| panel_rect.bottom());
    (bottom > panel_rect.top())
        .then(|| egui::Rect::from_min_max(panel_rect.min, egui::pos2(panel_rect.max.x, bottom)))
}

#[cfg(test)]
mod tests {
    use super::divider_rect;
    use eframe::egui::{Rect, pos2, vec2};

    #[test]
    fn panel_divider_stops_at_the_terminal_top_edge() {
        let panel = Rect::from_min_size(pos2(100.0, 20.0), vec2(200.0, 500.0));
        let terminal = Rect::from_min_size(pos2(0.0, 400.0), vec2(800.0, 120.0));

        let divider = divider_rect(panel, Some(terminal)).expect("visible divider");

        assert_eq!(divider.top(), 20.0);
        assert_eq!(divider.bottom(), 400.0);
    }

    #[test]
    fn panel_divider_is_hidden_if_terminal_covers_the_panel() {
        let panel = Rect::from_min_size(pos2(100.0, 20.0), vec2(200.0, 500.0));
        let terminal = Rect::from_min_size(pos2(0.0, 10.0), vec2(800.0, 600.0));

        assert!(divider_rect(panel, Some(terminal)).is_none());
    }
}
