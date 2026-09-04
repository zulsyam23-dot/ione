use std::path::PathBuf;

use egui_code_editor::Syntax;

pub struct Tab {
    pub path: Option<PathBuf>,
    pub content: String,
    pub syntax: Syntax,
    pub dirty: bool,
    pub name: String,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub goto_line: Option<usize>,
}

impl Tab {
    pub fn new(name: &str, content: &str, syntax: Syntax) -> Self {
        Self {
            path: None,
            content: content.to_string(),
            syntax,
            dirty: false,
            name: name.to_string(),
            cursor_line: 0,
            cursor_col: 0,
            goto_line: None,
        }
    }

    pub fn from_file(path: PathBuf, content: String, syntax: Syntax) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled".to_string());
        Self {
            path: Some(path),
            content,
            syntax,
            dirty: false,
            name,
            cursor_line: 0,
            cursor_col: 0,
            goto_line: None,
        }
    }

    pub fn display_name(&self) -> String {
        if self.dirty {
            format!("*{}", self.name)
        } else {
            self.name.clone()
        }
    }
}

pub struct TabManager {
    pub tabs: Vec<Tab>,
    pub active: usize,
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
        }
    }

    pub fn open_file(&mut self, path: PathBuf, content: String, syntax: Syntax) {
        for (i, tab) in self.tabs.iter().enumerate() {
            if tab.path.as_ref() == Some(&path) {
                self.active = i;
                return;
            }
        }
        self.tabs.push(Tab::from_file(path, content, syntax));
        self.active = self.tabs.len() - 1;
    }

    pub fn new_file(&mut self, name: &str, syntax: Syntax) {
        self.tabs.push(Tab::new(name, "", syntax));
        self.active = self.tabs.len() - 1;
    }

    pub fn close(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.tabs.remove(idx);
            if self.tabs.is_empty() {
                self.active = 0;
            } else if self.active >= self.tabs.len() {
                self.active = self.tabs.len() - 1;
            } else if self.active > idx {
                self.active -= 1;
            }
        }
    }

    pub fn close_active(&mut self) {
        if !self.tabs.is_empty() {
            self.close(self.active);
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    pub fn set_active(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active = idx;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn save_active(&mut self) -> Option<PathBuf> {
        let tab = self.tabs.get_mut(self.active)?;
        let path = tab.path.clone()?;
        std::fs::write(&path, &tab.content).ok()?;
        tab.dirty = false;
        Some(path)
    }

    pub fn save_all_dirty(&mut self) -> usize {
        let mut saved = 0;
        for tab in self.tabs.iter_mut() {
            if tab.dirty {
                if let Some(path) = &tab.path {
                    if std::fs::write(path, &tab.content).is_ok() {
                        tab.dirty = false;
                        saved += 1;
                    }
                }
            }
        }
        saved
    }

    pub fn save_active_as(&mut self, path: PathBuf) -> bool {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if std::fs::write(&path, &tab.content).is_ok() {
                tab.path = Some(path.clone());
                tab.name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "untitled".to_string());
                tab.dirty = false;
                return true;
            }
        }
        false
    }

    pub fn detect_syntax(path: &PathBuf) -> Syntax {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
        {
            "rs" => Syntax::rust(),
            "py" => Syntax::python(),
            "lua" => Syntax::lua(),
            "sh" | "bash" | "zsh" => Syntax::shell(),
            "sql" => Syntax::sql(),
            "asm" | "s" => Syntax::asm(),
            _ => Syntax::new("plain"),
        }
    }
}
