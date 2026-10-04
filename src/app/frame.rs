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

use eframe::egui::{self, Align, Layout};

use crate::core::icons::Icon;
use crate::core::style::{Palette, frame, header_label};

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

    fn plugin_context(&self) -> crate::plugin::PluginContext {
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
        if !(self.show_sidebar || self.git.visible || editing.is_some()) {
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

    fn show_sidebar_panel(
        &mut self,
        root_ui: &mut egui::Ui,
        commands: &mut Vec<AppCommand>,
    ) -> Option<egui::Rect> {
        if !self.show_sidebar {
            return None;
        }
        Some(
            egui::Panel::left("sidebar")
                .exact_size(230.0)
                .show_separator_line(false)
                .frame(frame(self.palette.panel, self.palette.border, 0, 8))
                .show(root_ui, |ui| {
                    self.show_sidebar(commands, ui);
                })
                .response
                .rect,
        )
    }

    fn show_terminal_panel(
        &mut self,
        root_ui: &mut egui::Ui,
        ctx: &egui::Context,
        commands: &mut Vec<AppCommand>,
    ) {
        // Keep the terminal's spawn dir in sync with the workspace root so
        // new shells open inside the opened folder.
        self.terminal.cwd = self.file_tree.root.clone();
        if !self.terminal.visible {
            return;
        }
        egui::Panel::bottom("terminal_panel")
            .resizable(true)
            .default_size(220.0)
            .min_size(80.0)
            .frame(frame(self.palette.panel, self.palette.border, 0, 0))
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(header_label(ui, "Terminal"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(8.0);
                        if ui.small_button("×").on_hover_text("Close").clicked() {
                            commands.push(AppCommand::ToggleTerminal);
                        }
                        if ui
                            .small_button("Copy")
                            .on_hover_text("Copy selection")
                            .clicked()
                        {
                            if let Some(text) = self.terminal.copy_selection() {
                                ctx.copy_text(text);
                            }
                        }
                    });
                });
                self.terminal.show(ui, &self.palette, ctx);
            });
    }

    fn show_git_panel(&mut self, root_ui: &mut egui::Ui, commands: &mut Vec<AppCommand>) {
        if !self.git.visible {
            return;
        }
        let rect = egui::Panel::right("git_panel")
            .exact_size(280.0)
            .show_separator_line(false)
            .frame(frame(self.palette.panel, self.palette.border, 0, 8))
            .show(root_ui, |ui| {
                self.git.show(
                    ui,
                    &mut self.icons,
                    &self.palette,
                    self.file_tree.root.as_ref(),
                    commands,
                );
            })
            .response
            .rect;
        // Persistent divider at the panel's left edge, painted last so no
        // panel content (rows, scrollbars) can ever cover it.
        root_ui.painter().vline(
            rect.left() + 0.5,
            rect.y_range(),
            egui::Stroke::new(1.0, self.palette.border),
        );
    }

    fn show_plugin_panel(&mut self, root_ui: &mut egui::Ui, commands: &mut Vec<AppCommand>) {
        if !self.plugins.any_dock_open() {
            return;
        }
        let rect = egui::Panel::right("plugin_dock")
            .exact_size(420.0)
            .show_separator_line(false)
            .frame(frame(self.palette.panel, self.palette.border, 0, 8))
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(header_label(ui, "AI Chat"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(4.0);
                        if self
                            .icons
                            .image_button(ui, Icon::Close, 16.0, "Close")
                            .clicked()
                        {
                            commands.push(AppCommand::ToggleAiChat);
                        }
                    });
                });
                ui.add_space(6.0);
                let mut pctx = self.plugin_context();
                self.plugins.on_dock(ui, &mut pctx);
            })
            .response
            .rect;
        root_ui.painter().vline(
            rect.left() + 0.5,
            rect.y_range(),
            egui::Stroke::new(1.0, self.palette.border),
        );
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
                let hidden_behind_veil = matches!(&self.loading, Some((ov, _)) if !ov.fullscreen)
                    && self.loader.busy();
                if self.tabs.is_empty() {
                    self.show_empty_state(ui, ctx);
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
        let sidebar_rect = self.show_sidebar_panel(root_ui, &mut commands);
        // Bottom status bar. Added FIRST so it sits innermost (against the
        // screen bottom); the terminal (added after) stacks above it.
        Self::show_status_bar(root_ui, &self.tabs, &self.branch(), &self.git);
        self.show_terminal_panel(root_ui, &ctx, &mut commands);
        self.show_git_panel(root_ui, &mut commands);
        self.show_plugin_panel(root_ui, &mut commands);
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
