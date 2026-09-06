use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use egui_code_editor::Syntax;

fn next_tab_uid() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A collapsed brace block: char index of the opening `{` and of its matching
/// closing `}` in the real buffer content. Lines strictly inside are hidden
/// behind a `⋯` marker row; the opening line stays visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fold {
    pub open: usize,
    pub close: usize,
}

pub struct Tab {
    /// Uniquely identifies this tab (egui state keys, undo/redo, cursor...).
    pub uid: u64,
    pub path: Option<PathBuf>,
    pub content: String,
    pub syntax: Syntax,
    pub dirty: bool,
    pub name: String,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub cursor_char: usize,
    pub goto_line: Option<usize>,
    pub pending_find: Option<(usize, usize)>,
    pub folds: Vec<Fold>,
    /// Active multi-select: seed word + live caret/selection ranges. `None`
    /// when inactive (see `editor::multi`).
    pub multi: Option<crate::editor::multi::MultiSel>,
    /// Built-in error/warning diagnostics, recomputed only when the content
    /// hash changes (see `diagnostics`).
    pub diagnostics: Vec<crate::diagnostics::Diagnostic>,
    /// Content hash the current diagnostics were computed from.
    pub diag_hash: u64,
}

impl Tab {
    pub fn new(name: &str, content: &str, syntax: Syntax) -> Self {
        Self {
            uid: next_tab_uid(),
            path: None,
            content: content.to_string(),
            syntax,
            dirty: false,
            name: name.to_string(),
            cursor_line: 0,
            cursor_col: 0,
            cursor_char: 0,
            goto_line: None,
            pending_find: None,
            folds: Vec::new(),
            multi: None,
            diagnostics: Vec::new(),
            diag_hash: 0,
        }
    }

    pub fn from_file(path: PathBuf, content: String, syntax: Syntax) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled".to_string());
        Self {
            uid: next_tab_uid(),
            path: Some(path),
            content,
            syntax,
            dirty: false,
            name,
            cursor_line: 0,
            cursor_col: 0,
            cursor_char: 0,
            goto_line: None,
            pending_find: None,
            folds: Vec::new(),
            multi: None,
            diagnostics: Vec::new(),
            diag_hash: 0,
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

    pub fn save_active(&mut self) -> Result<PathBuf, String> {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return Err("no active tab".to_string());
        };
        let Some(path) = tab.path.clone() else {
            return Err("no path yet — use Save As".to_string());
        };
        std::fs::write(&path, &tab.content)
            .map_err(|e| format!("failed to save {}: {e}", path.display()))?;
        tab.dirty = false;
        Ok(path)
    }

    /// Saves every dirty tab that has a path; returns (saved count, errors).
    pub fn save_all_dirty(&mut self) -> (usize, Vec<String>) {
        let mut saved = 0;
        let mut errors = Vec::new();
        for tab in self.tabs.iter_mut() {
            if tab.dirty {
                if let Some(path) = &tab.path {
                    if let Err(e) = std::fs::write(path, &tab.content) {
                        errors.push(format!("failed to save {}: {e}", path.display()));
                    } else {
                        tab.dirty = false;
                        saved += 1;
                    }
                }
            }
        }
        (saved, errors)
    }

    pub fn save_active_as(&mut self, path: PathBuf) -> Result<PathBuf, String> {
        std::fs::write(&path, &self.tabs[self.active].content)
            .map_err(|e| format!("failed to save {}: {e}", path.display()))?;
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.path = Some(path.clone());
            tab.name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "untitled".to_string());
            tab.dirty = false;
        }
        Ok(path)
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
            "js" | "jsx" | "mjs" | "cjs" => Syntax::new("js")
                .with_comment("//")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords([
                    "async", "await", "break", "case", "catch", "class", "const", "continue",
                    "default", "delete", "do", "else", "export", "extends", "finally", "for",
                    "from", "function", "get", "if", "import", "in", "instanceof", "let", "new",
                    "of", "return", "set", "static", "switch", "this", "throw", "try", "typeof",
                    "var", "void", "while", "yield",
                ])
                .with_special(["false", "null", "true", "undefined"]),
            "ts" | "tsx" | "mts" | "cts" => Syntax::new("typescript")
                .with_comment("//")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords([
                    "abstract", "any", "async", "await", "break", "case", "catch", "class",
                    "const", "continue", "declare", "default", "delete", "do", "else", "enum",
                    "export", "extends", "finally", "for", "from", "function", "get", "if",
                    "implements", "import", "in", "infer", "instanceof", "interface", "is",
                    "keyof", "let", "namespace", "never", "new", "of", "readonly", "return",
                    "set", "static", "switch", "this", "throw", "try", "type", "typeof", "var",
                    "void", "while", "yield",
                ])
                .with_types([
                    "boolean", "number", "object", "string", "symbol", "unknown", "void",
                ])
                .with_special(["false", "null", "true", "undefined"]),
            "html" | "htm" => Syntax::new("html").with_comment_multiline(["<!--", "-->"]),
            "css" => Syntax::new("css")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords(["@font-face", "@import", "@keyframes", "@media", "auto", "inherit", "none"]),
            "json" => Syntax::new("json")
                .with_comment_multiline(["/*", "*/"])
                .with_special(["false", "null", "true"]),
            "toml" => Syntax::new("toml")
                .with_comment("#")
                .with_special(["false", "true"]),
            "yaml" | "yml" => Syntax::new("yaml").with_comment("#"),
            "md" | "markdown" => Syntax::new("markdown"),
            "c" | "h" => Syntax::new("c")
                .with_comment("//")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords([
                    "auto", "break", "case", "char", "const", "continue", "default", "do",
                    "double", "else", "enum", "extern", "float", "for", "goto", "if", "int",
                    "long", "register", "return", "short", "signed", "sizeof", "static",
                    "struct", "switch", "typedef", "union", "unsigned", "void", "volatile",
                    "while",
                ])
                .with_types(["size_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t"]),
            "cc" | "cpp" | "cxx" | "hpp" => Syntax::new("cpp")
                .with_comment("//")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords([
                    "auto", "break", "case", "catch", "class", "const", "constexpr", "continue",
                    "default", "delete", "do", "else", "enum", "explicit", "extern", "for",
                    "friend", "if", "inline", "namespace", "new", "operator", "private",
                    "protected", "public", "return", "sizeof", "static", "struct", "switch",
                    "template", "this", "throw", "try", "typedef", "typename", "union",
                    "using", "virtual", "void", "while",
                ])
                .with_types(["bool", "char", "double", "float", "int", "long", "short", "size_t", "string", "vector"]),
            "go" => Syntax::new("go")
                .with_comment("//")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords([
                    "break", "case", "chan", "const", "continue", "default", "defer", "else",
                    "fallthrough", "for", "func", "go", "goto", "if", "import", "interface",
                    "map", "package", "range", "return", "select", "struct", "switch", "type",
                    "var",
                ])
                .with_special(["false", "nil", "true"]),
            "java" => Syntax::new("java")
                .with_comment("//")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords([
                    "abstract", "boolean", "break", "byte", "case", "catch", "char", "class",
                    "const", "continue", "default", "do", "double", "else", "enum", "extends",
                    "final", "finally", "float", "for", "goto", "if", "implements", "import",
                    "instanceof", "int", "interface", "long", "native", "new", "package",
                    "private", "protected", "public", "return", "short", "static", "strictfp",
                    "super", "switch", "synchronized", "this", "throw", "throws", "transient",
                    "try", "void", "volatile", "while",
                ])
                .with_special(["false", "null", "true"]),
            _ => Syntax::new("plain"),
        }
    }
}
