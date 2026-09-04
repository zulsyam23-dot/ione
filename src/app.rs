use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Align2, Color32, Layout, RichText};

use crate::file_tree::FileTree;
use crate::icons::{Icon, Icons};
use crate::menu;
use crate::outline::OutlinePanel;
use crate::search::SearchPanel;
use crate::style::{frame, header_label, Palette};
use crate::tabs::TabManager;
use crate::terminal::TerminalPanel;
use crate::theme::Theme;
use crate::loading::LoadingOverlay;

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
    pub palette: Palette,
    pub last_auto_save: Instant,
    pub outline_frac: f32,
    pub renaming: Option<RenameState>,
    pub logo: Option<egui::TextureHandle>,
    pub loading: Option<(LoadingOverlay, Instant)>,
    pub ctrl_k_pending: bool,
    pub editor_font: String,
    pub font_msg: Option<(String, Instant)>,
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
            palette: Palette::dark(),
            last_auto_save: Instant::now(),
            outline_frac: 0.4,
            renaming: None,
            logo: None,
            loading: None,
            ctrl_k_pending: false,
            editor_font: "JetBrains Mono".to_string(),
            font_msg: None,
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
}

impl eframe::App for EditorApp {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        let mut commands = Vec::new();

        if let Some((ov, t0)) = &mut self.loading {
            ov.show(&ctx);
            if t0.elapsed() >= Duration::from_secs(2) {
                self.loading = None;
            }
            return;
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

                if self.tabs.is_empty() {
                    self.show_empty_state(ui, &ctx);
                } else if let Some(tab) = self.tabs.active_tab_mut() {
                    crate::editor::show_editor(ui, tab, self.theme, self.palette.editor_bg);
                }
            });

        self.show_about_window(&ctx);
        self.show_rename_window(&ctx);

        self.process_commands(commands, &ctx);
        self.show_font_msg(&ctx);
    }
}

impl EditorApp {
    fn show_rename_window(&mut self, ctx: &egui::Context) {
        let mut state = match self.renaming.take() {
            Some(s) => s,
            None => return,
        };
        let mut submitted = false;
        let mut cancelled = false;
        egui::Window::new("Rename")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("New name:");
                let edit = ui.text_edit_singleline(&mut state.input);
                if !edit.has_focus() {
                    edit.request_focus();
                }
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        submitted = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                });
                if enter {
                    submitted = true;
                }
                if esc {
                    cancelled = true;
                }
            });
        if submitted {
            let p = state.path.clone();
            let n = state.input.clone();
            self.apply_rename(&p, &n);
            self.renaming = None;
        } else if cancelled {
            self.renaming = None;
        } else {
            self.renaming = Some(state);
        }
    }

    fn apply_rename(&mut self, path: &std::path::Path, new_name: &str) {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return;
        }
        let new_path = match path.parent() {
            Some(parent) => parent.join(new_name),
            None => return,
        };
        if new_path == path {
            return;
        }
        if std::fs::rename(path, &new_path).is_err() {
            return;
        }
        self.file_tree.refresh();
        for tab in self.tabs.tabs.iter_mut() {
            if tab.path.as_deref() == Some(path) {
                tab.path = Some(new_path.clone());
                tab.name = new_name.to_string();
            }
        }
    }

    fn delete_path(&mut self, path: &std::path::Path) {
        let ok = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        if ok.is_err() {
            return;
        }
        self.file_tree.refresh();
        self.tabs.tabs.retain(|tab| tab.path.as_deref() != Some(path));
    }

    fn auto_save(&mut self, ctx: &egui::Context) {        let has_dirty = self
            .tabs
            .tabs
            .iter()
            .any(|t| t.dirty && t.path.is_some());
        if has_dirty {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
            if self.last_auto_save.elapsed() >= std::time::Duration::from_secs(2) {
                self.tabs.save_all_dirty();
                self.last_auto_save = Instant::now();
            }
        }
    }

    fn branch(&self) -> String {        self.file_tree
            .root
            .as_ref()
            .and_then(|r| r.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "no project".to_string())
    }

    fn show_title_bar(&mut self, root_ui: &mut egui::Ui, commands: &mut Vec<AppCommand>) {
        egui::Panel::top("title_bar")
            .frame(frame(self.palette.bg, Color32::TRANSPARENT, 0, 3))
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    egui::menu::MenuBar::new().ui(ui, |ui| {
                        menu::show_menu_bar(ui, commands, &self.editor_font);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(8.0);
                        if self
                            .icons
                            .image_button(ui, Icon::Terminal, 14.0, "Toggle Terminal (Ctrl+`)")
                            .clicked()
                        {
                            commands.push(AppCommand::ToggleTerminal);
                        }
                    });
                });
            });
    }

    fn show_sidebar(&mut self, commands: &mut Vec<AppCommand>, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(header_label(ui, "Explorer"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self
                    .icons
                    .image_button(ui, Icon::Refresh, 14.0, "Refresh")
                    .clicked()
                {
                    commands.push(AppCommand::RefreshFileTree);
                }
            });
        });
        ui.add_space(8.0);

        let total_h = ui.available_height();
        let total_w = ui.available_width();
        let tree_h = ((total_h - HANDLE) * (1.0 - self.outline_frac)).max(40.0);
        let outline_h = (total_h - tree_h - HANDLE).max(40.0);

        ui.allocate_ui(egui::vec2(total_w, tree_h), |ui| {
            self.file_tree.show(ui, &mut self.icons, &self.palette, commands);
        });

        drag_vertical_splitter(ui, self.palette.border, &mut self.outline_frac, total_h);

        ui.allocate_ui(egui::vec2(total_w, outline_h), |ui| {
            let (content, syntax) = match self.tabs.active_tab() {
                Some(tab) => (tab.content.clone(), tab.syntax.clone()),
                None => (String::new(), egui_code_editor::Syntax::new("plain")),
            };
            if !content.is_empty() {
                if let Some(line) = self.outline.show(ui, &self.palette, &content, &syntax) {
                    if let Some(tab) = self.tabs.active_tab_mut() {
                        tab.goto_line = Some(line);
                    }
                }
            }
        });
    }

    fn show_empty_state(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Center the block strictly within the area that remains after the
        // terminal/status panels, so it can never spill over them.
        let avail = ui.available_rect_before_wrap();

        if self.logo.is_none() {
            self.logo = Some(load_logo(ctx));
        }

        let logo_sz = 72.0_f32;
        let gap = 16.0_f32;
        let title_h = 40.0_f32;
        let sub_h = 24.0_f32;
        let total = logo_sz + gap + title_h + 10.0 + sub_h;
        let cx = avail.center().x;
        let mut y = avail.center().y - total / 2.0;

        if let Some(tex) = &self.logo {
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
            self.palette.text,
        );
        y += title_h + 10.0;

        let sub_rc = egui::Rect::from_center_size(
            egui::pos2(cx, y + sub_h / 2.0),
            egui::vec2(500.0, sub_h),
        );
        ui.painter().text(
            sub_rc.center(),
            Align2::CENTER_CENTER,
            "Open a file or create a new one to get started.",
            egui::FontId::proportional(15.0),
            self.palette.text_muted,
        );
    }

    fn show_status_bar(root_ui: &mut egui::Ui, tabs: &TabManager, branch: &str) {
        let p = Palette::dark();
        egui::Panel::bottom("status_bar")
            .frame(frame(p.bg, p.border, 0, 5))
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(header_label(ui, &format!(" {}", branch)));
                    ui.separator();
                    if let Some(tab) = tabs.active_tab() {
                        let lines = tab.content.chars().filter(|&c| c == '\n').count() + 1;
                        ui.label(
                            RichText::new(format!("Ln {}, Col {}  ({lines} lines)", tab.cursor_line + 1, tab.cursor_col + 1))
                                .color(p.text),
                        );
                        ui.separator();
                        ui.label(RichText::new("UTF-8").color(p.text_muted));
                    } else {
                        ui.label(RichText::new("No file open").color(p.text_muted));
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(tab) = tabs.active_tab() {
                            let lang = if tab.path.is_none() {
                                "Plain Text".to_string()
                            } else {
                                tab.syntax.language().to_string()
                            };
                            ui.label(RichText::new(lang).color(p.text));
                            ui.separator();
                        }
                        ui.separator();
                        ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).color(p.text_muted));
                    });
                });
            });
    }

    fn show_tab_bar(tabs: &mut TabManager, ui: &mut egui::Ui, p: Palette) {
        if tabs.is_empty() {
            return;
        }

        let mut switch_to: Option<usize> = None;
        let mut close_idx: Option<usize> = None;

        egui::Panel::top("tab_bar")
            .frame(frame(ui.visuals().extreme_bg_color, Color32::TRANSPARENT, 0, 5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    for (i, tab) in tabs.tabs.iter().enumerate() {
                        let active = i == tabs.active;
                        let text = tab.display_name();
                        let label = if tab.dirty {
                            format!("● {text}")
                        } else {
                            text
                        };
                        let color = if active { p.text } else { p.text_muted };
                        let fill = if active { p.panel_active } else { Color32::TRANSPARENT };
                        let tab_frame = egui::Frame::NONE
                            .fill(fill)
                            .corner_radius(0.0);
                        let response = tab_frame.show(ui, |ui| {
                            ui.add(
                                egui::Button::new(RichText::new(&label).color(color)),
                            )
                        })
                        .inner;

                        if response.clicked() {
                            switch_to = Some(i);
                        }
                        response.context_menu(|ui| {
                            if ui.button("Close").clicked() {
                                close_idx = Some(i);
                                ui.close();
                            }
                        });
                        if response.middle_clicked() {
                            close_idx = Some(i);
                        }
                        ui.add_space(2.0);
                    }
                });
            });

        if let Some(idx) = switch_to {
            tabs.set_active(idx);
        }
        if let Some(idx) = close_idx {
            tabs.close(idx);
        }
    }

    fn show_breadcrumbs(&self, ui: &mut egui::Ui, commands: &mut Vec<AppCommand>) {
        let Some(tab) = self.tabs.active_tab() else {
            return;
        };
        let Some(path) = &tab.path else {
            return;
        };
        egui::Panel::top("breadcrumbs")
            .frame(frame(self.palette.panel, Color32::TRANSPARENT, 0, 8))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut cumulative = PathBuf::new();
                    for comp in path.components() {
                        use std::path::Component;
                        match comp {
                            Component::RootDir => {
                                cumulative.push(std::path::MAIN_SEPARATOR.to_string());
                            }
                            Component::CurDir | Component::ParentDir => continue,
                            Component::Normal(os) => {
                                cumulative.push(os);
                                let seg = cumulative.clone();
                                let label = os.to_string_lossy().to_string();
                                let is_file = cumulative == *path;
                                let color = if is_file {
                                    self.palette.text_muted
                                } else {
                                    self.palette.text
                                };
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new(&label).color(color))
                                            .frame(false),
                                    )
                                    .on_hover_text(seg.to_string_lossy().to_string())
                                    .clicked()
                                {
                                    commands.push(AppCommand::SetRoot(seg));
                                }
                                ui.label(RichText::new("/").color(self.palette.text_muted));
                            }
                            Component::Prefix(_) => {}
                        }
                    }
                });
            });
    }

    fn show_about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }

        let p = self.palette;
        egui::Window::new("About ione")
            .collapsible(false)
            .resizable(true)
            .default_width(440.0)
            .show(ctx, |ui| {
                if self.logo.is_none() {
                    self.logo = Some(load_logo(ctx));
                }

                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    // Header: logo + name + tagline.
                    ui.horizontal(|ui| {
                        if let Some(tex) = &self.logo {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::hover());
                            ui.painter().image(
                                tex.id(),
                                rect,
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                        }
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("ione")
                                    .size(22.0)
                                    .strong()
                                    .color(p.text),
                            );
                            ui.label(
                                egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                                    .size(12.0)
                                    .color(p.text_muted),
                            );
                        });
                    });
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(
                            "ione is a lightweight, multifunctional code editor with an \
                             integrated terminal, built for a fast and distraction-free workflow.",
                        )
                        .size(13.0)
                        .color(p.text_muted),
                    );

                    // Features.
                    ui.add_space(14.0);
                    ui.label(header_label(ui, "Features"));
                    ui.add_space(4.0);
                    for feat in [
                        "Multi-tab code editor with syntax highlighting",
                        "Integrated multi-session PowerShell terminal",
                        "File explorer with expand / collapse & context menu",
                        "Find & Replace with live results",
                        "Dark & light themes (pure black / white based)",
                        "Open Folder flow with recent-file navigation",
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("•").color(p.accent));
                            ui.label(egui::RichText::new(feat).size(12.5).color(p.text));
                        });
                    }

                    // Shortcuts.
                    ui.add_space(14.0);
                    ui.label(header_label(ui, "Shortcuts"));
                    ui.add_space(4.0);
                    let shortcuts: [(&str, &str); 9] = [
                        ("New File", "Ctrl+N"),
                        ("Open File", "Ctrl+O"),
                        ("Open Folder", "Ctrl+K Ctrl+O"),
                        ("Save", "Ctrl+S"),
                        ("Save As", "Ctrl+Shift+S"),
                        ("Close Tab", "Ctrl+W"),
                        ("Find & Replace", "Ctrl+H"),
                        ("Toggle File Explorer", "Ctrl+L"),
                        ("Toggle Terminal", "Ctrl+`"),
                    ];
                    for (name, sc) in shortcuts {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(name).size(12.5).color(p.text));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(egui::RichText::new(sc).color(p.text_muted));
                            });
                        });
                    }
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Tab cycle: Ctrl+Tab / Ctrl+Shift+Tab")
                            .size(12.5)
                            .color(p.text_muted),
                    );

                    // Tech stack.
                    ui.add_space(14.0);
                    ui.label(header_label(ui, "Tech Stack"));
                    ui.add_space(4.0);
                    for tech in [
                        "Rust (edition 2024)",
                        "egui / eframe 0.36",
                        "egui_code_editor",
                        "portable-pty + vt100 (terminal)",
                        "resvg + egui_extras (icons)",
                    ] {
                        ui.label(egui::RichText::new(format!("• {tech}")).size(12.5).color(p.text));
                    }

                    // Footer.
                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "© ione — Rust code editor. Built with ♥ and egui."
                        ))
                        .size(11.5)
                        .color(p.text_muted),
                    );
                    ui.add_space(4.0);
                });

                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    if ui
                        .add_sized([100.0, 28.0], egui::Button::new("Close"))
                        .clicked()
                    {
                        self.show_about = false;
                    }
                });
            });
    }

    fn show_font_msg(&mut self, ctx: &egui::Context) {
        let Some((msg, t0)) = self.font_msg.clone() else {
            return;
        };
        let dur = 3.5_f64;
        let left = dur - t0.elapsed().as_secs_f64();
        if left <= 0.0 {
            self.font_msg = None;
            return;
        }
        let alpha = ((left / dur).clamp(0.0, 1.0) as f32) * 255.0;
        egui::Area::new(egui::Id::new("font_msg"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(self.palette.panel_active)
                    .stroke(egui::Stroke::new(1.0, self.palette.border))
                    .corner_radius(0)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(msg)
                                .color(self.palette.text.gamma_multiply(alpha)),
                        );
                    });
            });
        ctx.request_repaint();
    }

    fn process_commands(&mut self, commands: Vec<AppCommand>, ctx: &egui::Context) {
        for cmd in commands {
            match cmd {
                AppCommand::NewFile => {
                    let count = self
                        .tabs
                        .tabs
                        .iter()
                        .filter(|t| t.path.is_none())
                        .count();
                    let name = if count == 0 {
                        "untitled".to_string()
                    } else {
                        format!("untitled-{}", count + 1)
                    };
                    self.tabs.new_file(&name, egui_code_editor::Syntax::new("plain"));
                }
                AppCommand::OpenFile => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("All Files", &["*"])
                        .pick_file()
                    {
                        self.open_path(path);
                    }
                }
                AppCommand::OpenFilePath(path) => {
                    self.open_path(path);
                }
                AppCommand::OpenFolder => {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        self.file_tree.set_root(folder);
                    }
                }
                AppCommand::SetRoot(path) => {
                    if path.is_dir() {
                        self.file_tree.set_root(path);
                    }
                }
                AppCommand::RenamePath(path) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.renaming = Some(RenameState {
                        path,
                        input: name,
                    });
                }
                AppCommand::DeletePath(path) => {
                    self.delete_path(&path);
                }
                AppCommand::CopyPath(path) => {
                    ctx.copy_text(path.to_string_lossy().to_string());
                }
                AppCommand::Save => {
                    let needs_dialog = self
                        .tabs
                        .active_tab()
                        .map(|t| t.path.is_none())
                        .unwrap_or(true);
                    if needs_dialog {
                        self.save_as_dialog();
                    } else {
                        self.tabs.save_active();
                    }
                }
                AppCommand::SaveAs => {
                    self.save_as_dialog();
                }
                AppCommand::CloseTab => {
                    self.tabs.close_active();
                }
                AppCommand::NextTab => {
                    if !self.tabs.is_empty() {
                        let next = (self.tabs.active + 1) % self.tabs.tabs.len();
                        self.tabs.set_active(next);
                    }
                }
                AppCommand::PrevTab => {
                    if !self.tabs.is_empty() {
                        let prev = if self.tabs.active == 0 {
                            self.tabs.tabs.len() - 1
                        } else {
                            self.tabs.active - 1
                        };
                        self.tabs.set_active(prev);
                    }
                }
                AppCommand::ToggleSearch => {
                    self.search.toggle();
                }
                AppCommand::ToggleSidebar => {
                    self.show_sidebar = !self.show_sidebar;
                }
                AppCommand::ToggleTerminal => {
                    self.terminal.toggle(ctx);
                }
                AppCommand::RefreshFileTree => {
                    self.file_tree.refresh();
                }
                AppCommand::SetTheme(theme) => {
                    self.theme = theme;
                    apply_egui_theme(ctx, theme);
                }
                AppCommand::SetEditorFont(name) => {
                    self.editor_font = name.clone();
                    match crate::fonts::apply_font(ctx, &name) {
                        Ok(()) => {
                            self.font_msg = Some((format!("Font: {name}"), Instant::now()));
                        }
                        Err(e) => {
                            self.font_msg = Some((e, Instant::now()));
                        }
                    }
                }
                AppCommand::FindNext(query) => {
                    if let Some(tab) = self.tabs.active_tab_mut() {
                        find_next_in_content(&mut tab.content, &query, self.search.case_sensitive);
                    }
                }
                AppCommand::FindPrev(query) => {
                    if let Some(tab) = self.tabs.active_tab_mut() {
                        find_prev_in_content(&mut tab.content, &query, self.search.case_sensitive);
                    }
                }
                AppCommand::Replace(query, replacement) => {
                    if let Some(tab) = self.tabs.active_tab_mut() {
                        replace_first(
                            &mut tab.content,
                            &query,
                            &replacement,
                            self.search.case_sensitive,
                        );
                        tab.dirty = true;
                    }
                }
                AppCommand::ReplaceAll(query, replacement) => {
                    if let Some(tab) = self.tabs.active_tab_mut() {
                        replace_all_in_content(
                            &mut tab.content,
                            &query,
                            &replacement,
                            self.search.case_sensitive,
                        );
                        tab.dirty = true;
                    }
                }
                AppCommand::About => {
                    self.show_about = !self.show_about;
                }
                AppCommand::Exit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let syntax = TabManager::detect_syntax(&path);
            self.tabs.open_file(path, content, syntax);
        }
    }

    fn save_as_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new().save_file() {
            self.tabs.save_active_as(path);
        }
    }
}

const HANDLE: f32 = 6.0;

fn load_logo(ctx: &egui::Context) -> egui::TextureHandle {
    let bytes = include_bytes!("..\\assets\\icons\\1770523897143.ico");
    let img = image::load_from_memory(bytes)
        .expect("Failed to load logo")
        .into_rgba8();
    let (w, h) = img.dimensions();
    let color = egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], img.as_raw());
    ctx.load_texture("ione_logo", color, egui::TextureOptions::LINEAR)
}

fn drag_vertical_splitter(ui: &mut egui::Ui, color: Color32, frac: &mut f32, total_h: f32) {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), HANDLE),
        egui::Sense::drag(),
    );
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    ui.painter().rect(
        rect,
        0.0,
        if resp.hovered() || resp.dragged() {
            color
        } else {
            Color32::TRANSPARENT
        },
        egui::Stroke::NONE,
        egui::StrokeKind::Inside,
    );
    let dy = resp.drag_delta().y;
    if dy != 0.0 && total_h > 0.0 {
        *frac = (*frac - dy / total_h).clamp(0.1, 0.9);
    }
}

fn apply_egui_theme(ctx: &egui::Context, theme: Theme) {
    crate::style::apply_style(
        ctx,
        if theme.is_dark() {
            Palette::dark()
        } else {
            Palette::light()
        },
    );
}

fn find_next_in_content(content: &str, query: &str, case_sensitive: bool) -> Option<usize> {
    if query.is_empty() {
        return None;
    }
    if case_sensitive {
        content.find(query)
    } else {
        content.to_lowercase().find(&query.to_lowercase())
    }
}

fn find_prev_in_content(content: &str, query: &str, case_sensitive: bool) -> Option<usize> {
    if query.is_empty() {
        return None;
    }
    if case_sensitive {
        content.rfind(query)
    } else {
        content.to_lowercase().rfind(&query.to_lowercase())
    }
}

fn replace_first(content: &mut String, query: &str, replacement: &str, case_sensitive: bool) {
    if query.is_empty() {
        return;
    }
    if let Some(idx) = find_next_in_content(content, query, case_sensitive) {
        let end = idx + query.len();
        content.replace_range(idx..end, replacement);
    }
}

fn replace_all_in_content(
    content: &mut String,
    query: &str,
    replacement: &str,
    case_sensitive: bool,
) {
    if query.is_empty() {
        return;
    }
    if case_sensitive {
        *content = content.replace(query, replacement);
    } else {
        let lower = content.to_lowercase();
        let query_lower = query.to_lowercase();
        let mut result = String::new();
        let mut last_end = 0;
        for (start, _) in lower.match_indices(&query_lower) {
            result.push_str(&content[last_end..start]);
            result.push_str(replacement);
            last_end = start + query.len();
        }
        result.push_str(&content[last_end..]);
        *content = result;
    }
}
