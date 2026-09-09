use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui;

use crate::loading::LoadingOverlay;
use crate::tabs::TabManager;

use super::EditorApp;
use super::utils::{
    apply_egui_theme, documents_dir, find_next_range, find_prev_range, replace_all_in_content,
    replace_first,
};
use super::{
    AppCommand, NamingKind, NamingState, QuickOpen, RenameState,
};

/// `Some(new.join(rest))` when `p` lives under `old` (itself included).
fn remap_under(p: &Path, old: &Path, new: &Path) -> Option<PathBuf> {
    p.strip_prefix(old).ok().map(|rest| new.join(rest))
}

impl EditorApp {
    pub(super) fn auto_save(&mut self, ctx: &egui::Context) {
        let has_dirty = self
            .tabs
            .tabs
            .iter()
            .any(|t| t.dirty && t.path.is_some());
        if has_dirty {
            // Wake once, at the next 2s deadline — not every 500ms on a timer.
            let interval = std::time::Duration::from_secs(2);
            let elapsed = self.last_auto_save.elapsed();
            if elapsed >= interval {
                let (_, errors) = self.tabs.save_all_dirty();
                if !errors.is_empty() {
                    self.font_msg = Some((errors.join("; "), Instant::now()));
                }
                self.last_auto_save = Instant::now();
            } else {
                ctx.request_repaint_after(interval - elapsed);
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
        let Some(parent) = path.parent() else { return };
        let new_path = parent.join(new_name);
        if new_path == path {
            return;
        }
        if let Err(e) = std::fs::rename(path, &new_path) {
            self.font_msg = Some((format!("Rename gagal: {e}"), Instant::now()));
            return;
        }
        // Keep the workspace alive when the root itself is renamed.
        if self.file_tree.root.as_deref() == Some(path) {
            self.file_tree.root = Some(new_path.clone());
        }
        // Folders keep their expansion state; open tabs below the renamed
        // folder (or the renamed file itself) keep pointing at new_path,
        // otherwise Save writes into a deleted path.
        for p in self.file_tree.expanded.iter_mut() {
            if let Some(np) = remap_under(p, path, &new_path) {
                *p = np;
            }
        }
        self.file_tree.refresh();
        for tab in self.tabs.tabs.iter_mut() {
            let Some(tp) = tab.path.clone() else { continue };
            let Some(np) = remap_under(&tp, path, &new_path) else { continue };
            tab.path = Some(np.clone());
            tab.name = np
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            // Extension may have changed; re-pick the highlight + completion.
            tab.syntax = TabManager::detect_syntax(&np);
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
                    let prefill = if count == 0 {
                        "untitled.rs".to_string()
                    } else {
                        format!("untitled-{}.rs", count + 1)
                    };
                    self.naming = Some(NamingState {
                        input: prefill,
                        kind: NamingKind::File,
                        parent: None,
                    });
                }
                AppCommand::NewFolder(parent) => {
                    let n = self.file_tree.count_dirs(parent.as_deref());
                    self.naming = Some(NamingState {
                        input: if n == 0 {
                            "untitled-folder".to_string()
                        } else {
                            format!("untitled-folder-{}", n + 1)
                        },
                        kind: NamingKind::Folder,
                        parent,
                    });
                }
                AppCommand::NewFileIn(dir) => {
                    let n = self.file_tree.count_files(Some(&dir));
                    self.naming = Some(NamingState {
                        input: if n == 0 {
                            "untitled.rs".to_string()
                        } else {
                            format!("untitled-{}.rs", n + 1)
                        },
                        kind: NamingKind::File,
                        parent: Some(dir),
                    });
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
                AppCommand::OpenRecent(path) => {
                    if !path.is_file() {
                        // The entry went missing; drop it so the menu stays honest.
                        self.recent_files.retain(|p| p != &path);
                        crate::settings::save_recents(&self.recent_files);
                        continue;
                    }
                    self.open_path(path);
                }
                AppCommand::QuickOpen => {
                    if self.quick_open.is_some() {
                        self.quick_open = None;
                    } else {
                        // The file tree only lists expanded folders' children;
                        // that is the palette's reach (recent files back it up).
                        let mut paths: Vec<PathBuf> = self
                            .file_tree
                            .entries
                            .iter()
                            .filter(|e| !e.is_dir)
                            .map(|e| e.path.clone())
                            .collect();
                        paths.sort();
                        self.quick_open = Some(QuickOpen {
                            query: String::new(),
                            selected: 0,
                            paths,
                        });
                    }
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
                        self.settings.theme = Some(theme.name().to_string());
                        self.settings.save();
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
                    self.settings.font = Some(name);
                    self.settings.save();
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
                        // An active multi-select is always extended by its seed
                        // word: the caret can't move away while it runs (any
                        // click/arrow cancels it), so this survives batch edits.
                        if let Some(m) = &tab.multi {
                            let word = m.word.clone();
                            let after = m.ranges.iter().map(|&(s, _)| s).max().unwrap_or(0);
                            let sel = m.ranges.clone();
                            if let Some(next) = crate::editor::multi::next_occurrence(
                                &tab.content,
                                &word,
                                after,
                                &sel,
                            ) {
                                if let Some(m) = &mut tab.multi {
                                    m.ranges.push(next);
                                    m.ranges.sort_by_key(|&(s, _)| s);
                                    m.ranges.dedup();
                                }
                            }
                            continue;
                        }
                        // Fresh seed: the occurrence under the caret — not the
                        // file's first occurrence.
                        let byte_pos = tab
                            .content
                            .chars()
                            .take(tab.cursor_char)
                            .map(char::len_utf8)
                            .sum();
                        let Some(word) = crate::editor::multi::word_at(&tab.content, byte_pos)
                        else {
                            continue;
                        };
                        if let Some(first) =
                            crate::editor::multi::occurrence_at(&tab.content, &word, byte_pos)
                        {
                            tab.multi = Some(crate::editor::multi::MultiSel {
                                word,
                                ranges: vec![first],
                            });
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
        // Splash for heavy opens: set before the read when the file is big
        // enough to stall on disk, and after the read when it spans many
        // lines (keeps the pre-existing `>= 500 lines` behavior).
        let splash = |s: &mut Self| {
            if let Some(ov) = LoadingOverlay::new_loading_file() {
                s.loading = Some((ov, Instant::now()));
            }
        };
        let heavy_bytes = std::fs::metadata(&path).is_ok_and(|m| m.len() >= 50_000);
        if heavy_bytes {
            splash(self);
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            if !heavy_bytes && content.lines().count() >= 500 {
                splash(self);
            }
            let syntax = TabManager::detect_syntax(&path);
            self.tabs.open_file(path.clone(), content, syntax);
            crate::settings::push_recent(&mut self.recent_files, path);
        }
    }

    pub(super) fn save_as_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new().save_file() {
            if let Err(e) = self.tabs.save_active_as(path) {
                self.font_msg = Some((e, Instant::now()));
            } else {
                // A file created on disk must appear in the explorer right away.
                self.file_tree.refresh();
            }
        }
    }

    /// Create the file the "New File" dialog submitted. With a folder open the
    /// file is written to disk immediately (so it shows up in the explorer, VS
    /// Code style); otherwise the tab stays in-memory until Save As gives it a
    /// path. A name that already exists is opened, never overwritten. `parent`
    /// overrides the workspace root so a context-menu "New File in folder"
    /// lands exactly where the user right-clicked.
    pub(super) fn create_new_file(&mut self, name: &str, parent: Option<&Path>) {
        let name = name.trim();
        // Only a plain file name is accepted: no separators, no `.` / `..`.
        let ok = !name.is_empty()
            && Path::new(name).components().count() == 1
            && !name.contains(['/', '\\', ':']);
        if !ok {
            return;
        }
        let dir = parent
            .map(Path::to_path_buf)
            .or_else(|| self.file_tree.root.clone())
            .or_else(documents_dir);
        let Some(dir) = dir else {
            let syntax = TabManager::detect_syntax(&PathBuf::from(name));
            self.tabs.new_file(name, syntax);
            return;
        };
        let path = dir.join(name);
        if path.is_file() {
            if parent.is_none() && self.file_tree.root.is_none() {
                self.file_tree.set_root(dir.clone());
            }
            self.open_path(path);
            return;
        }
        if path.is_dir() {
            return;
        }
        let _ = std::fs::write(&path, "");
        if parent.is_none() && self.file_tree.root.is_none() {
            self.file_tree.set_root(dir.clone());
        } else {
            // Reveal the target folder if it isn't the tree root already.
            if self.file_tree.root.as_deref() != Some(dir.as_path())
                && !self.file_tree.expanded.contains(&dir)
            {
                self.file_tree.expanded.push(dir.clone());
            }
            self.file_tree.refresh();
        }
        self.open_path(path);
    }

    /// Create the folder the "New Folder" dialog submitted, under `parent`
    /// (or the workspace root when None). No-op if it already exists.
    pub(super) fn create_new_folder(&mut self, name: &str, parent: Option<PathBuf>) {
        let name = name.trim();
        let ok = !name.is_empty()
            && Path::new(name).components().count() == 1
            && !name.contains(['/', '\\', ':']);
        if !ok {
            return;
        }
        let root = parent
            .clone()
            .or_else(|| self.file_tree.root.clone())
            .or_else(documents_dir);
        let Some(root) = root else { return };
        let path = root.join(name);
        if path.exists() {
            self.file_tree.refresh();
            return;
        }
        if std::fs::create_dir(&path).is_err() {
            return;
        }
        // Make the new folder visible right away: expand its parent.
        if let Some(p) = &parent {
            if !self.file_tree.expanded.contains(p) {
                self.file_tree.expanded.push(p.clone());
            }
        }
        self.file_tree.refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::remap_under;
    use std::path::Path;

    #[test]
    fn remap_under_moves_paths_below_renamed_entry() {
        let old = Path::new("C:\\proj\\src\\gui");
        let new = Path::new("C:\\proj\\src\\ui");
        assert_eq!(
            remap_under(Path::new("C:\\proj\\src\\gui\\main.rs"), old, new),
            Some(Path::new("C:\\proj\\src\\ui\\main.rs").to_path_buf())
        );
        assert_eq!(
            remap_under(Path::new("C:\\proj\\src\\gui"), old, new),
            Some(Path::new("C:\\proj\\src\\ui").to_path_buf())
        );
        assert_eq!(
            remap_under(Path::new("C:\\proj\\src\\core\\x.rs"), old, new),
            None
        );
    }
}