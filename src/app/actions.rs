use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui;

use crate::loading::LoadingOverlay;
use crate::tabs::TabManager;

use super::EditorApp;
use super::utils::{apply_egui_theme, find_next_range, find_prev_range, replace_all_in_content, replace_first};
use super::{AppCommand, RenameState};

impl EditorApp {
    pub(super) fn auto_save(&mut self, ctx: &egui::Context) {
        let has_dirty = self
            .tabs
            .tabs
            .iter()
            .any(|t| t.dirty && t.path.is_some());
        if has_dirty {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
            if self.last_auto_save.elapsed() >= std::time::Duration::from_secs(2) {
                let (_, errors) = self.tabs.save_all_dirty();
                if !errors.is_empty() {
                    self.font_msg = Some((errors.join("; "), Instant::now()));
                }
                self.last_auto_save = Instant::now();
            }
        }
    }

    pub(super) fn branch(&self) -> String {
        self.file_tree
            .root
            .as_ref()
            .and_then(|r| r.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "no project".to_string())
    }

    pub(super) fn apply_rename(&mut self, path: &Path, new_name: &str) {
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

    fn delete_path(&mut self, path: &Path) {
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

    pub(super) fn process_commands(&mut self, commands: Vec<AppCommand>, ctx: &egui::Context) {
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
                    } else if let Err(e) = self.tabs.save_active() {
                        self.font_msg = Some((e, Instant::now()));
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
                    if theme == self.theme {
                        self.pending_theme = None;
                        continue;
                    }
                    if theme.is_beta() && self.pending_theme != Some(theme) {
                        self.pending_theme = Some(theme);
                    } else {
                        self.pending_theme = None;
                        self.theme = theme;
                        apply_egui_theme(ctx, theme);
                    }
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
                        let case = self.search.case_sensitive;
                        tab.pending_find =
                            find_next_range(&tab.content, &query, case, tab.cursor_char);
                    }
                }
                AppCommand::FindPrev(query) => {
                    if let Some(tab) = self.tabs.active_tab_mut() {
                        let case = self.search.case_sensitive;
                        tab.pending_find =
                            find_prev_range(&tab.content, &query, case, tab.cursor_char);
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
                        tab.folds.clear();
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
                        tab.folds.clear();
                    }
                }
                AppCommand::Fold => {
                    if let Some(tab) = self.tabs.active_tab_mut() {
                        let (mask, _) =
                            crate::editor::styling::mask_and_links(&tab.content, &tab.syntax);
                        let chars: Vec<char> = tab.content.chars().collect();
                        let scan = crate::guides::analyze_brackets(&chars, &mask);
                        let opens: Vec<usize> = crate::editor::folds::foldable_opens(
                            &tab.content,
                            &scan.brace_pairs,
                        );
                        // `cursor_line` is 1-based (gutter numbering); fold
                        // helpers take a 0-based line.
                        let line = tab.cursor_line.saturating_sub(1);
                        crate::editor::folds::toggle_fold(&mut tab.folds, &opens, &tab.content, line);
                    }
                }
                AppCommand::Unfold => {
                    if let Some(tab) = self.tabs.active_tab_mut() {
                        let line = tab.cursor_line.saturating_sub(1);
                        crate::editor::folds::unfold_covering(&mut tab.folds, &tab.content, line);
                    }
                }
                AppCommand::MultiSelectNext => {
                    if let Some(tab) = self.tabs.active_tab_mut() {
                        let byte_pos = tab
                            .content
                            .chars()
                            .take(tab.cursor_char)
                            .map(char::len_utf8)
                            .sum();
                        let Some(word) = crate::editor::multi::word_at(&tab.content, byte_pos)
                        else {
                            tab.multi = None;
                            continue;
                        };
                        // Fresh seed: select the first occurrence of the word.
                        if tab.multi.as_ref().is_none_or(|m| m.word != word) {
                            let first = crate::editor::multi::occurrences(&tab.content, &word)
                                .into_iter()
                                .next();
                            tab.multi = Some(crate::editor::multi::MultiSel {
                                word,
                                ranges: first.into_iter().collect(),
                            });
                            continue;
                        }
                        // Same seed: add the next unselected occurrence.
                        let after = tab
                            .multi
                            .as_ref()
                            .map(|m| m.ranges.iter().map(|&(s, _)| s).max().unwrap_or(0))
                            .unwrap_or(0);
                        let sel = &tab.multi.as_ref().unwrap().ranges;
                        if let Some(next) = crate::editor::multi::next_occurrence(
                            &tab.content,
                            &word,
                            after,
                            sel,
                        ) {
                            if let Some(m) = &mut tab.multi {
                                m.ranges.push(next);
                                m.ranges.sort_by_key(|&(s, _)| s);
                                m.ranges.dedup();
                            }
                        }
                    }
                }
                AppCommand::ToggleBracketGuides => {
                    self.bracket_guides = !self.bracket_guides;
                }
                AppCommand::ToggleBracketColorize => {
                    self.colorize_brackets = !self.colorize_brackets;
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

    pub(super) fn open_path(&mut self, path: PathBuf) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            // Heavy files (hundreds/thousands of lines) show the editor-area
            // loading splash so the open doesn't feel like a hang.
            if content.lines().count() >= 500 {
                if let Some(ov) = LoadingOverlay::new_loading_file() {
                    self.loading = Some((ov, Instant::now()));
                }
            }
            let syntax = TabManager::detect_syntax(&path);
            self.tabs.open_file(path, content, syntax);
        }
    }

    pub(super) fn save_as_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new().save_file() {
            if let Err(e) = self.tabs.save_active_as(path) {
                self.font_msg = Some((e, Instant::now()));
            }
        }
    }
}