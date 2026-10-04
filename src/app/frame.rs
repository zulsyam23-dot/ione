//! Frame composition root: one method per UI region, driven by `ui()` below.
//!
//! The per-frame pipeline, in order:
//! 1. startup splash (full-screen, blocks everything),
//! 2. drain worker results (file loader, git, plugins),
//! 3. sync palette/settings/plugins,
//! 4. shortcuts + auto-save,
//! 5. paint the panels (title bar, sidebar, status bar, terminal, git,
//!    plugin dock) and the central editor,
//! 6. popups, overlays, and finally the command queue.

use std::time::{Duration, Instant};

use eframe::egui;

use crate::core::style::Palette;

use super::{AppCommand, EditorApp, menu, utils};

impl EditorApp {
    /// Full-screen startup splash: paint it and swallow the whole frame.
    fn show_startup_splash(&mut self, ctx: &egui::Context) -> bool {
        if let Some((ov, t0)) = &mut self.loading
            && ov.fullscreen
        {
            ov.show(ctx, None);
            if t0.elapsed() >= Duration::from_secs(2) {
                self.loading = None;
            }
            return true;
        }
        false
    }

    pub(crate) fn plugin_context(&self) -> crate::plugin::PluginContext {
        crate::plugin::PluginContext {
            palette: self.palette,
            bracket_guides: self.bracket_guides,
            colorize_brackets: self.colorize_brackets,
        }
    }

    /// Apply persisted settings on the first frame, then refresh the palette
    /// and let plugins adjust live UI state before anything paints.
    fn sync_ui_state(&mut self, ctx: &egui::Context) {
        if !self.settings_applied {
            self.settings_applied = true;
            if self.editor_font != "JetBrains Mono" {
                let _ = crate::core::fonts::apply_font(ctx, &self.editor_font);
            }
            utils::apply_egui_theme(ctx, self.theme);
        }
        self.palette = if self.theme.is_dark() {
            Palette::dark()
        } else {
            Palette::light()
        };
        self.icons.set_color(self.palette.text);

        let mut pctx = self.plugin_context();
        self.plugins.on_tick(&mut pctx);
        self.palette = pctx.palette;
        self.bracket_guides = pctx.bracket_guides;
        self.colorize_brackets = pctx.colorize_brackets;

        let mut pctx_ui = self.plugin_context();
        self.plugins.on_ui(ctx, &mut pctx_ui);
    }

    /// Keep explorer tint / status-bar git snippet / gutter change markers
    /// current. Scans run off the UI thread; the same scan answers the gutter
    /// markers for the file being edited.
    fn pump_git(&mut self, ctx: &egui::Context) {
        let editing = self
            .tabs
            .tabs
            .get(self.tabs.active)
            .and_then(|t| t.path.clone());
        if !(self.panels.explorer_visible || self.git.visible || editing.is_some()) {
            return;
        }
        let busy = self.git.busy();
        self.git.refresh(
            self.file_tree.root.as_deref(),
            Instant::now(),
            false,
            editing.as_deref(),
        );
        if self.git.poll() {
            ctx.request_repaint();
        }
        // Hand the markers to the open file; `mark_rev` keeps this to a copy
        // per scan rather than per frame.
        let mark_rev = self.git.mark_rev();
        if let Some(tab) = self.tabs.tabs.get_mut(self.tabs.active)
            && tab.path.is_some()
            && tab.cache.git_marks_rev != mark_rev
        {
            let lines = tab
                .path
                .as_deref()
                .map(|path| self.git.marks_for(path).to_vec())
                .unwrap_or_default();
            tab.cache.git_marks = lines;
            tab.cache.git_marks_rev = mark_rev;
        }
        // A scan/diff is in flight: poll more often so results land promptly.
        if busy {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }

    /// The central editor area. Returns `true` on frames where the editor
    /// body actually painted (the splash retires only then).
    fn show_editor_area(
        &mut self,
        root_ui: &mut egui::Ui,
        ctx: &egui::Context,
        commands: &mut Vec<AppCommand>,
    ) -> bool {
        let mut editor_drawn = false;
        egui::CentralPanel::default_margins()
            .frame(egui::Frame::NONE.fill(self.palette.bg))
            .show(root_ui, |ui| {
                Self::show_tab_bar(&mut self.tabs, ui, self.palette);

                let content = self
                    .tabs
                    .active_tab()
                    .map(|t| t.content.as_str())
                    .unwrap_or_default();

                self.show_breadcrumbs(ui, commands);

                if self.search.visible {
                    self.search.show(
                        ui,
                        &mut self.icons,
                        content,
                        self.file_tree.root.as_ref(),
                        commands,
                    );
                }

                let guides = self.guides();
                let palette = self.palette;
                // While the veil is up and the worker is still reading, the
                // editor body is pure cost: it is covered, and laying out a
                // large file every frame is what made the spinner stutter.
                let hidden_behind_veil =
                    matches!(&self.loading, Some((ov, _)) if !ov.fullscreen) && self.loader.busy();
                if self.tabs.is_empty() {
                    crate::panels::PanelManager::show_empty_state(self, ui, ctx);
                    editor_drawn = true;
                } else if hidden_behind_veil {
                    // Nothing to do: the next frame paints it.
                } else if let Some(tab) = self.tabs.active_tab_mut() {
                    crate::editor::show_editor(
                        ui,
                        tab,
                        self.theme,
                        palette.editor_bg,
                        &guides,
                        &palette,
                        &mut self.icons,
                    );
                    editor_drawn = true;
                }

                // Editor-area splash while a file is being read on a worker thread.
                if let Some((ov, _)) = &mut self.loading
                    && !ov.fullscreen
                {
                    ov.show(ctx, Some(ui.max_rect()));
                }
            });
        editor_drawn
    }
}

impl eframe::App for EditorApp {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        let mut commands = Vec::new();

        if self.show_startup_splash(&ctx) {
            return;
        }

        // Editor-area splash is visual only: shed every input event for its
        // duration so nothing (typing, shortcuts, clicks) lands behind the veil.
        if self.loading.is_some() {
            ctx.input_mut(|i| i.events.clear());
        }

        // Land whatever the file-loader workers finished, before anything reads
        // the tabs: the tab (and its off-thread analysis) must exist by the
        // time the editor panel below is laid out.
        self.pump_loader(&ctx);
        self.sync_ui_state(&ctx);
        self.auto_save(&ctx);
        menu::handle_shortcuts(&ctx, &mut commands, &mut self.ctrl_k_pending);
        self.pump_git(&ctx);

        self.show_title_bar(root_ui, &mut commands);
        // The panel manager owns the layout order and delegates each panel to
        // its isolated renderer module.
        let sidebar_rect =
            crate::panels::PanelManager::show_all(self, root_ui, &ctx, &mut commands);
        let editor_drawn = self.show_editor_area(root_ui, &ctx, &mut commands);

        // Retire the editor splash only on a frame that actually painted the
        // editor with the loaded content: the first frame of a large file is
        // the expensive one (full-file layout), and paying it behind the veil
        // is what keeps the app from freezing right after the GIF disappears.
        if self.splash_can_retire(editor_drawn) {
            self.loading = None;
        }

        self.show_about_window(&ctx);
        self.show_rename_window(&ctx);
        self.show_new_file_window(&ctx);
        self.show_theme_confirm(&ctx, &mut commands);
        self.show_quick_open(&ctx, &mut commands);
        self.show_goto_line_window(&ctx);

        // Persistent divider at the explorer's right edge, painted last so no
        // panel content (tree rows, scrollbars) can ever cover it.
        if let Some(r) = sidebar_rect {
            root_ui.painter().vline(
                r.right() - 0.5,
                r.y_range(),
                egui::Stroke::new(1.0, self.palette.border),
            );
        }

        self.process_commands(commands, &ctx);
        self.show_font_msg(&ctx);
    }
}
