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
mod chrome;
mod explorer_bar;
mod popups;
mod utils;

#[derive(Debug, Clone)]
pub enum AppCommand {
    NewFile,
    NewFileIn(PathBuf),
    NewFolder(Option<PathBuf>),
    OpenFile,
    OpenFilePath(PathBuf),
    OpenFolder,
    SetRoot(PathBuf),
    OpenRecent(PathBuf),
    QuickOpen,
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
    pub naming: Option<NamingState>,
    pub logo: Option<egui::TextureHandle>,
    pub loading: Option<(LoadingOverlay, Instant)>,
    pub ctrl_k_pending: bool,
    pub editor_font: String,
    pub font_msg: Option<(String, Instant)>,
    pub pending_theme: Option<Theme>,
    /// Lightweight "open file" palette (Ctrl+P), open when `Some`.
    pub quick_open: Option<QuickOpen>,
    pub recent_files: Vec<PathBuf>,
    pub settings: crate::settings::Settings,
    pub settings_applied: bool,
}

/// The Ctrl+P file palette: live query + selection over the workspace files.
pub struct QuickOpen {
    pub query: String,
    pub selected: usize,
    pub paths: Vec<PathBuf>,
}

pub struct RenameState {
    pub path: PathBuf,
    pub input: String,
}

/// In-progress "New File"/"New Folder" dialog: the typed name.
pub struct NamingState {
    pub input: String,
    pub kind: NamingKind,
    /// Where a new folder goes (root if None).
    pub parent: Option<PathBuf>,
}

#[derive(Clone, Copy)]
pub enum NamingKind {
    File,
    Folder,
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
            naming: None,
            logo: None,
            loading: None,
            ctrl_k_pending: false,
            editor_font: "JetBrains Mono".to_string(),
            font_msg: None,
            pending_theme: None,
            quick_open: None,
            recent_files: Vec::new(),
            settings: crate::settings::Settings::default(),
            settings_applied: false,
        }
    }
}

impl EditorApp {
    pub fn new() -> Self {
        let mut app = Self::default();
        // Persisted preferences (theme/font) and the recent-files list.
        let loaded = crate::settings::load();
        app.settings = crate::settings::Settings {
            theme: loaded.theme.clone(),
            font: loaded.font.clone(),
        };
        if let Some(t) = &loaded.theme {
            if let Some(theme) = crate::theme::Theme::ALL.iter().find(|x| x.name() == t) {
                app.theme = *theme;
            }
        }
        if let Some(f) = &loaded.font {
            if crate::fonts::FONTS.iter().any(|(n, _)| n == f) {
                app.editor_font = f.clone();
            }
        }
        app.recent_files = crate::settings::load_recents();
        // Default workspace = the user's Documents, so new files/folders are
        // easy to find instead of living in an invisible in-memory state.
        if let Some(docs) = utils::documents_dir() {
            app.file_tree.set_root(docs);
        }
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

    /// Ctrl+P file palette: a top-centered window with a query box and the
    /// workspace files. Enter/↑/↓/Esc are read from the raw events (the
    /// single-line box consumes Enter as "done"), and a click opens too.
    fn show_quick_open(&mut self, ctx: &egui::Context, commands: &mut Vec<AppCommand>) {
        let Some(q) = &mut self.quick_open else { return };
        let root = self.file_tree.root.clone();
        let (esc, enter, up, down) = ctx.input(|i| {
            let mut r = (false, false, false, false);
            for e in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    ..
                } = e
                {
                    match key {
                        egui::Key::Escape => r.0 = true,
                        egui::Key::Enter => r.1 = true,
                        egui::Key::ArrowUp => r.2 = true,
                        egui::Key::ArrowDown => r.3 = true,
                        _ => {}
                    }
                }
            }
            r
        });
        let mut chose: Option<PathBuf> = None;
        egui::Window::new("Quick Open (Ctrl+P)")
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 40.0])
            .show(ctx, |ui| {
                if up {
                    q.selected = q.selected.saturating_sub(1);
                }
                if down {
                    q.selected = q.selected.saturating_add(1);
                }
                let query = q.query.to_lowercase();
                let matches: Vec<&PathBuf> = q
                    .paths
                    .iter()
                    .filter(|p| {
                        query.is_empty()
                            || p
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_lowercase()
                                .contains(&query)
                    })
                    .collect();
                if q.selected >= matches.len() {
                    q.selected = 0;
                }
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut q.query)
                        .hint_text("Type a file name to open…")
                        .desired_width(f32::INFINITY),
                );
                if !resp.has_focus() {
                    resp.request_focus();
                }
                if enter && !matches.is_empty() {
                    let sel = q.selected.min(matches.len() - 1);
                    chose = Some(matches[sel].clone());
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .id_salt("quick_open_scroll")
                    .show(ui, |ui| {
                        if matches.is_empty() {
                            ui.label("No files — open a folder first (Ctrl+K Ctrl+O).");
                        }
                        for (i, p) in matches.iter().take(300).enumerate() {
                            let rel = root
                                .as_ref()
                                .and_then(|r| p.strip_prefix(r).ok())
                                .map(|rp| rp.to_string_lossy().to_string())
                                .unwrap_or_else(|| p.to_string_lossy().to_string());
                            if ui.selectable_label(q.selected == i, rel).clicked() {
                                chose = Some((*p).clone());
                            }
                        }
                    });
            });
        if esc {
            self.quick_open = None;
        } else if let Some(p) = chose {
            commands.push(AppCommand::OpenRecent(p));
            self.quick_open = None;
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

        // Editor-area splash is visual only: shed every input event for its
        // duration so nothing (typing, shortcuts, clicks) lands behind the veil.
        if self.loading.is_some() {
            ctx.input_mut(|i| i.events.clear());
        }

        // Apply the persisted theme/font on the first usable frame (egui's
        // visuals are set up before the app runs, so this overrides them).
        if !self.settings_applied {
            self.settings_applied = true;
            if self.editor_font != "JetBrains Mono" {
                let _ = crate::fonts::apply_font(&ctx, &self.editor_font);
            }
            utils::apply_egui_theme(&ctx, self.theme);
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
        let sidebar_rect = if self.show_sidebar {
            Some(
                egui::Panel::left("sidebar")
                    .exact_size(230.0)
                    .show_separator_line(false)
                    .frame(frame(self.palette.panel, self.palette.border, 0, 8))
                    .show(root_ui, |ui| {
                        self.show_sidebar(&mut commands, ui);
                    })
                    .response
                    .rect,
            )
        } else {
            None
        };

        // Bottom status bar. Added FIRST so it sits innermost (against the
        // screen bottom); the terminal (added after) stacks above it.
        Self::show_status_bar(root_ui, &self.tabs, &self.branch());

        // Bottom terminal, as a root panel so egui reserves its space natively.
        // (A nested Panel::bottom inside CentralPanel does not shrink the editor's
        // available rect, which is what let the editor content show through/over it.)
        // Keep the terminal's spawn dir in sync with the workspace root so
        // new shells open inside the opened folder.
        self.terminal.cwd = self.file_tree.root.clone();

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
                    .map(|t| t.content.as_str())
                    .unwrap_or_default();

                self.show_breadcrumbs(ui, &mut commands);

                if self.search.visible {
                    self.search.show(
                        ui,
                        &mut self.icons,
                        content,
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
                if let Some((ov, _)) = &mut self.loading {
                    if !ov.fullscreen {
                        ov.show(&ctx, Some(ui.max_rect()));
                        if ov.done(Duration::from_millis(600)) {
                            self.loading = None;
                        }
                    }
                }
            });

        self.show_about_window(&ctx);
        self.show_rename_window(&ctx);
        self.show_new_file_window(&ctx);
        self.show_theme_confirm(&ctx, &mut commands);
        self.show_quick_open(&ctx, &mut commands);

        // Persistent divider at the explorer's right edge, painted last so no
        // panel content (tree rows, scrollbars) can ever cover it.
        if let Some(r) = sidebar_rect {
            root_ui
                .painter()
                .vline(r.right() - 0.5, r.y_range(), egui::Stroke::new(1.0, self.palette.border));
        }

        self.process_commands(commands, &ctx);
        self.show_font_msg(&ctx);
    }
}