use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self};

use crate::workspace::file_tree::FileTree;
use crate::git::GitPanel;
use crate::editor::guides::EditorOverlay;
use crate::core::icons::Icons;
use crate::workspace::loader::FileLoader;
use crate::app::loading::LoadingOverlay;
use crate::workspace::outline::OutlinePanel;
use crate::workspace::search::SearchPanel;
use crate::core::style::Palette;
use crate::workspace::tabs::TabManager;
use crate::terminal::TerminalPanel;
use crate::core::theme::Theme;

pub mod actions;
mod frame;
pub mod chrome;
pub mod explorer_bar;
pub mod popups;
pub mod utils;
pub mod loading;
pub mod menu;

/// Minimum time the loading splash stays up once its load has landed, so a
/// quick open still shows the GIF instead of flashing it for a couple of frames.
const SPLASH_MIN: Duration = Duration::from_millis(450);

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
    ToggleGit,
    ToggleAiChat,
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
    pub git: GitPanel,
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
    /// `(overlay, when it was scheduled)`. `fullscreen` during startup, an
    /// editor-area splash while a heavy file is being read on a worker thread.
    pub loading: Option<(LoadingOverlay, Instant)>,
    /// Off-thread reads for heavy files, so opening one never blocks a frame.
    pub loader: FileLoader,
    pub ctrl_k_pending: bool,
    pub editor_font: String,
    pub font_msg: Option<(String, Instant)>,
    pub pending_theme: Option<Theme>,
    /// Lightweight "open file" palette (Ctrl+P), open when `Some`.
    pub quick_open: Option<QuickOpen>,
    pub recent_files: Vec<PathBuf>,
    pub settings: crate::core::settings::Settings,
    pub plugins: crate::plugin::PluginRegistry,
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
            git: GitPanel::new(),
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
            loader: FileLoader::new(),
            ctrl_k_pending: false,
            editor_font: "JetBrains Mono".to_string(),
            font_msg: None,
            pending_theme: None,
            quick_open: None,
            recent_files: Vec::new(),
            settings: crate::core::settings::Settings::default(),
            settings_applied: false,
            plugins: crate::plugin::PluginRegistry::new(),
        }
    }
}

impl EditorApp {
    pub fn new() -> Self {
        let mut app = Self::default();
        // Persisted preferences (theme/font) and the recent-files list.
        let loaded = crate::core::settings::load();
        app.settings = crate::core::settings::Settings {
            theme: loaded.theme.clone(),
            font: loaded.font.clone(),
        };
        if let Some(t) = &loaded.theme {
            if let Some(theme) = crate::core::theme::Theme::ALL.iter().find(|x| x.name() == t) {
                app.theme = *theme;
            }
        }
        if let Some(f) = &loaded.font {
            if crate::core::fonts::FONTS.iter().any(|(n, _)| n == f) {
                app.editor_font = f.clone();
            }
        }
        app.recent_files = crate::core::settings::load_recents();

        let mut pctx = crate::plugin::PluginContext {
            palette: app.palette,
            bracket_guides: app.bracket_guides,
            colorize_brackets: app.colorize_brackets,
        };
        app.plugins.register(
            Box::new(crate::plugin::builtin::ai_chat::AiChatPlugin::new()),
            &mut pctx,
        );
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

    /// Apply whatever finished on the file-loader workers: open each tab that
    /// arrived (its text passes ride along, so the editor has nothing left to
    /// scan) and surface read errors. The splash is retired by `ui`, on a frame
    /// that also painted the editor — see the note there.
    fn pump_loader(&mut self, ctx: &egui::Context) {
        let (loaded, errors) = self.loader.poll();
        let landed = !loaded.is_empty() || !errors.is_empty();
        for file in loaded {
            self.tabs
                .open_loaded(file.path.clone(), file.content, file.analysis);
            crate::core::settings::push_recent(&mut self.recent_files, file.path);
        }
        for e in errors {
            self.font_msg = Some((e, Instant::now()));
        }
        if self.loader.busy() {
            // Poll again soon so the result lands (and the splash keeps
            // animating) without waiting on unrelated input.
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if landed {
            // Something just landed: keep the frames coming so the editor gets
            // painted (behind the veil) and the splash can retire.
            ctx.request_repaint();
        }
    }

    /// Whether the editor splash may retire now. It needs all three: the load is
    /// done, the minimum display time has passed, and this frame actually painted
    /// the editor — the first frame of a large file is the expensive one, and it
    /// has to happen behind the veil rather than after it.
    fn splash_can_retire(&self, editor_drawn: bool) -> bool {
        let Some((ov, _)) = &self.loading else {
            return false;
        };
        !ov.fullscreen && editor_drawn && !self.loader.busy() && ov.done(SPLASH_MIN)
    }

    /// Ctrl+P file palette: a top-centered window with a query box and the
    /// workspace files. Enter/↑/↓/Esc are read from the raw events (the
    /// single-line box consumes Enter as "done"), and a click opens too.
    fn show_quick_open(&mut self, ctx: &egui::Context, commands: &mut Vec<AppCommand>) {
        let Some(q) = &mut self.quick_open else {
            return;
        };
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
                            || p.file_name()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A ~2 MB file, written to a temp dir.
    fn big_file(name: &str) -> (PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("ione-open-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        // Repetitive but syntactically real Rust: braces, strings, symbols —
        // enough structure that every text pass has work to do.
        let unit = "fn unit() {\n    let s = \"a { b\";\n    println!(\"{s}\");\n}\n";
        let content = unit.repeat(2_000_000 / unit.len() + 64);
        let path = dir.join(name);
        std::fs::write(&path, &content).expect("write big file");
        (path, content)
    }

    fn app_without_startup_splash() -> EditorApp {
        let mut app = EditorApp::new();
        // Drop the one-shot startup splash; this test is about file opens.
        app.loading = None;
        app
    }

    /// Paint one frame of the editor area, the way `ui()` does: this is what
    /// starts the veil's display clock (`LoadingOverlay::done` counts from the
    /// first painted frame, not from when it was scheduled).
    fn paint_one_frame(ctx: &egui::Context, app: &mut EditorApp) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            if let Some((ov, _)) = &mut app.loading {
                ov.show(ctx, Some(ui.max_rect()));
            }
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn a_small_file_also_opens_off_thread_behind_the_splash() {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        let mut app = app_without_startup_splash();
        let recents_before = app.recent_files.clone();
        let dir = std::env::temp_dir().join(format!("ione-open-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("small.rs");
        std::fs::write(&path, "fn main() {}\n").expect("write");

        app.open_path(path.clone());

        // Every open goes through the worker, so every open shows the GIF.
        assert!(app.tabs.is_empty(), "nothing is read on the UI thread");
        assert!(app.loader.busy());
        let (ov, _) = app.loading.as_ref().expect("veil is up");
        assert!(!ov.fullscreen, "the chrome must stay visible");
        assert_eq!(
            ov.caption.as_ref().map(|(t, _)| t.as_str()),
            Some("small.rs")
        );

        for _ in 0..400 {
            paint_one_frame(&ctx, &mut app);
            app.pump_loader(&ctx);
            if !app.tabs.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(app.tabs.tabs.len(), 1);
        assert_eq!(app.tabs.tabs[0].content, "fn main() {}\n");

        crate::core::settings::save_recents(&recents_before);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_big_file_loads_off_thread_behind_the_splash() {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        let mut app = app_without_startup_splash();
        let recents_before = app.recent_files.clone();
        let (path, content) = big_file("heavy.rs");

        app.open_path(path.clone());

        // The tab is *not* there yet — nothing was read on this thread.
        assert!(app.tabs.is_empty());
        assert!(app.loader.busy());
        let (ov, _) = app.loading.as_ref().expect("veil is up");
        assert!(!ov.fullscreen, "the chrome must stay visible");
        let (title, detail) = ov.caption.as_ref().expect("caption");
        assert_eq!(title, "heavy.rs");
        assert!(detail.contains("MB"), "caption shows the size: {detail}");

        // Pump frames the way `ui()` does until the worker lands.
        for _ in 0..400 {
            paint_one_frame(&ctx, &mut app);
            app.pump_loader(&ctx);
            if !app.tabs.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert_eq!(app.tabs.tabs.len(), 1, "the worker never delivered");
        assert_eq!(app.tabs.tabs[0].content, content);
        assert_eq!(app.tabs.tabs[0].path.as_deref(), Some(path.as_path()));
        assert!(
            app.recent_files.contains(&path),
            "opened files go to recents"
        );
        // The analysis rode along, ready for the editor's first frame.
        assert!(app.tabs.tabs[0].pending_analysis.is_some());
        assert!(!app.loader.busy());

        crate::core::settings::save_recents(&recents_before);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_veil_survives_at_least_its_minimum_display_time() {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        let mut app = app_without_startup_splash();
        // A finished load with nothing in flight: only the minimum display
        // time and a painted editor frame stand between the veil and its
        // dismissal.
        app.loading = Some((
            LoadingOverlay::editor_area(("x.rs".into(), "2.0 MB".into())).expect("splash"),
            Instant::now(),
        ));

        paint_one_frame(&ctx, &mut app);
        app.pump_loader(&ctx);
        assert!(
            app.loading.is_some(),
            "a fast open must not flash the overlay for one frame"
        );
        assert!(
            !app.splash_can_retire(true),
            "the minimum display time has to hold first"
        );

        std::thread::sleep(SPLASH_MIN + Duration::from_millis(20));
        assert!(
            app.splash_can_retire(true),
            "after the minimum time a painted frame may retire it"
        );
        assert!(
            !app.splash_can_retire(false),
            "but never on a frame that skipped the editor"
        );
    }

    #[test]
    fn reopening_an_open_tab_does_not_reload_it() {
        let mut app = app_without_startup_splash();
        let (path, content) = big_file("reopen.rs");
        let recents_before = app.recent_files.clone();

        app.open_path(path.clone());
        for _ in 0..400 {
            app.pump_loader(&egui::Context::default());
            if !app.tabs.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(app.tabs.tabs.len(), 1);

        // A second open of the same path focuses the existing tab and starts
        // no new work — no veil, no duplicate tab.
        app.loading = None;
        app.open_path(path.clone());
        assert_eq!(app.tabs.tabs.len(), 1);
        assert_eq!(app.tabs.tabs[0].content, content);
        assert!(!app.loader.busy());
        assert!(app.loading.is_none());

        crate::core::settings::save_recents(&recents_before);
        let _ = std::fs::remove_file(&path);
    }
}
