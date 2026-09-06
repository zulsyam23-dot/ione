use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Layout};

use crate::file_tree::FileTree;
use crate::guides::EditorOverlay;
use crate::icons::Icons;
use crate::loading::LoadingOverlay;
use crate::menu;
use crate::outline::OutlinePanel;
use crate::search::SearchPanel;
use crate::style::{frame, header_label, Palette};
use crate::tabs::TabManager;
use crate::terminal::TerminalPanel;
use crate::theme::Theme;

mod actions;
mod panels;
mod utils;

#[derive(Debug, Clone)]
pub enum AppCommand {
    NewFile,
    OpenFile,
    OpenFilePath(PathBuf),
    OpenFolder,
    SetRoot(PathBuf),
    RenamePath(PathBuf),
    DeletePath(PathBuf),
    CopyPath(PathBuf),
    Save,
    SaveAs,
    CloseTab,
    NextTab,
    PrevTab,
    ToggleSearch,
    ToggleSidebar,
    ToggleTerminal,
    RefreshFileTree,
    SetTheme(Theme),
    SetEditorFont(String),
    FindNext(String),
    FindPrev(String),
    Replace(String, String),
    ReplaceAll(String, String),
    ToggleBracketGuides,
    ToggleBracketColorize,
    Fold,
    Unfold,
    MultiSelectNext,
    About,
    Exit,
}

pub struct EditorApp {
    pub tabs: TabManager,
    pub file_tree: FileTree,
    pub search: SearchPanel,
    pub outline: OutlinePanel,
    pub terminal: TerminalPanel,
    pub theme: Theme,
    pub icons: Icons,
    pub show_sidebar: bool,
    pub show_about: bool,
    pub bracket_guides: bool,
    pub colorize_brackets: bool,
    pub palette: Palette,
    pub last_auto_save: Instant,
    pub outline_frac: f32,
    pub renaming: Option<RenameState>,
    pub logo: Option<egui::TextureHandle>,
    pub loading: Option<(LoadingOverlay, Instant)>,
    pub ctrl_k_pending: bool,
    pub editor_font: String,
    pub font_msg: Option<(String, Instant)>,
    pub pending_theme: Option<Theme>,
}

pub struct RenameState {
    pub path: PathBuf,
    pub input: String,
}

impl Default for EditorApp {
    fn default() -> Self {
        Self {
            tabs: TabManager::new(),
            file_tree: FileTree::new(),
            search: SearchPanel::new(),
            outline: OutlinePanel::new(),
            terminal: TerminalPanel::new(),
            theme: Theme::default(),
            icons: Icons::new(),
            show_sidebar: true,
            show_about: false,
            bracket_guides: true,
            colorize_brackets: true,
            palette: Palette::dark(),
            last_auto_save: Instant::now(),
            outline_frac: 0.4,
            renaming: None,
            logo: None,
            loading: None,
            ctrl_k_pending: false,
            editor_font: "JetBrains Mono".to_string(),
            font_msg: None,
            pending_theme: None,
        }
    }
}

impl EditorApp {
    pub fn new() -> Self {
        let mut app = Self::default();
        if let Some(ov) = LoadingOverlay::new() {
            app.loading = Some((ov, Instant::now()));
        }
        app
    }

    pub(super) fn guides(&self) -> EditorOverlay {
        EditorOverlay {
            bracket_guides: self.bracket_guides,
            colorize_brackets: self.colorize_brackets,
        }
    }
}

impl eframe::App for EditorApp {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        let mut commands = Vec::new();

        // Full-screen startup splash blocks everything until it finishes.
        if let Some((ov, t0)) = &mut self.loading {
            if ov.fullscreen {
                ov.show(&ctx, None);
                if t0.elapsed() >= Duration::from_secs(2) {
                    self.loading = None;
                }
                return;
            }
        }

        self.palette = if self.theme.is_dark() {
            Palette::dark()
        } else {
            Palette::light()
        };
        self.icons.set_color(self.palette.text);

        self.auto_save(&ctx);
        menu::handle_shortcuts(&ctx, &mut commands, &mut self.ctrl_k_pending);

        self.show_title_bar(root_ui, &mut commands);

        // Left sidebar (file explorer).
        if self.show_sidebar {
            egui::Panel::left("sidebar")
                .exact_size(230.0)
                .show_separator_line(false)
                .frame(frame(self.palette.panel, self.palette.border, 0, 8))
                .show(root_ui, |ui| {
                    self.show_sidebar(&mut commands, ui);
                });
        }

        // Bottom status bar. Added FIRST so it sits innermost (against the
        // screen bottom); the terminal (added after) stacks above it.
        Self::show_status_bar(root_ui, &self.tabs, &self.branch());

        // Bottom terminal, as a root panel so egui reserves its space natively.
        // (A nested Panel::bottom inside CentralPanel does not shrink the editor's
        // available rect, which is what let the editor content show through/over it.)
        if self.terminal.visible {
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
                        });
                    });
                    self.terminal.show(ui, &self.palette, &ctx);
                });
        }

        egui::CentralPanel::default_margins()
            .frame(egui::Frame::NONE.fill(self.palette.bg))
            .show(root_ui, |ui| {
                Self::show_tab_bar(&mut self.tabs, ui, self.palette);

                let content = self
                    .tabs
                    .active_tab()
                    .map(|t| t.content.clone())
                    .unwrap_or_default();

                self.show_breadcrumbs(ui, &mut commands);

                if self.search.visible {
                    self.search.show(
                        ui,
                        &mut self.icons,
                        &content,
                        self.file_tree.root.as_ref(),
                        &mut commands,
                    );
                }

                let guides = self.guides();
                let palette = self.palette;
                if self.tabs.is_empty() {
                    self.show_empty_state(ui, &ctx);
                } else if let Some(tab) = self.tabs.active_tab_mut() {
                    crate::editor::show_editor(ui, tab, self.theme, palette.editor_bg, &guides, &palette, &mut self.icons);
                }

                // Editor-area splash while a heavy file is being opened.
                if let Some((ov, _t0)) = &mut self.loading {
                    if !ov.fullscreen {
                        ov.show(&ctx, Some(ui.max_rect()));
                    }
                }
            });

        self.show_about_window(&ctx);
        self.show_rename_window(&ctx);
        self.show_theme_confirm(&ctx, &mut commands);

        self.process_commands(commands, &ctx);
        self.show_font_msg(&ctx);
    }
}