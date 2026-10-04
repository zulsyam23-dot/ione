use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use egui_code_editor::Syntax;
use egui_code_editor::highlighting::Links;

use crate::editor::guides::brackets::BracketScan;

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
    /// Live autocomplete popup state; `None` while hidden (see `completion`).
    pub completion: Option<crate::editor::completion::CompletionState>,
    /// Content hash the cached passes were computed from (see `refresh_cache`).
    pub diag_hash: u64,
    /// Text-derived passes for `content`, computed on a worker thread when the
    /// file was opened (see `loader`). The first `refresh_cache` consumes it —
    /// and only when the content hash and syntax still match, so an edit that
    /// lands first can never pick up stale scan data. `None` afterwards.
    pub pending_analysis: Option<Box<crate::workspace::loader::Analysis>>,
    /// Memoized per-character artifacts. Everything is keyed on the content
    /// hash: recomputed once per content change, reused for frames after that.
    /// `view` is additionally keyed on the fold set (see `fold_view_cache`).
    pub cache: TabCache,
}

/// Per-character buffers recomputed only when the file changes, not per frame.
pub struct TabCache {
    pub diagnostics: Vec<crate::editor::diagnostics::Diagnostic>,
    pub symbols: Vec<crate::workspace::outline::Symbol>,
    pub scan: BracketScan,
    /// String/comment char mask (`true` = inside a string/comment), shared
    /// with auto-close and bracket logic without re-lexing.
    pub mask: Vec<bool>,
    pub fold_opens: Vec<usize>,
    pub fold_rows_key: Option<(u64, u64)>,
    pub fold_rows: Vec<usize>,
    pub diag_rows_key: Option<(u64, u64)>,
    pub diag_rows: Vec<(usize, crate::editor::diagnostics::Severity)>,
    pub filtered_pairs_galley: Option<std::sync::Arc<eframe::egui::Galley>>,
    pub filtered_pairs: Vec<crate::editor::guides::Pair>,
    /// Fold view keyed on `(content hash, fold fingerprint, view)`. Shared, not
    /// owned: idle frames take an `Arc` clone instead of copying the display
    /// string and the display->real map (megabytes on a large file).
    pub fold_view_cache: Option<(u64, u64, std::sync::Arc<crate::editor::folds::FoldView>)>,
    /// Style key covering theme/overlay/palette/syntax for the mask+links.
    pub style_key: u64,
    /// Cached editor galley: `(style_key, text_hash, font_generation,
    /// pixels_per_point_bits, galley, links, styled)`. Replaying the identical
    /// galley (Arc clone) on later frames skips the LayoutJob clone +
    /// re-layout that used to run per frame.
    pub job_cache: Option<(
        u64,
        u64,
        u64,
        u32,
        std::sync::Arc<eframe::egui::Galley>,
        Links,
        crate::editor::styling::Styled,
    )>,
    /// Gutter text reused across frames and rebuilt only when content/folds change.
    pub gutter_key: Option<(u64, u64)>,
    pub gutter_text: String,
    /// Changed lines git reports for this file, copied from the Source Control
    /// panel's scan. `git_marks_rev` is the panel revision they came from, so
    /// the gutter picks up new markers exactly once per scan.
    pub git_marks: Vec<usize>,
    pub git_marks_rev: u64,
}

impl Default for TabCache {
    fn default() -> Self {
        Self {
            diagnostics: Vec::new(),
            symbols: Vec::new(),
            scan: BracketScan::default(),
            mask: Vec::new(),
            fold_opens: Vec::new(),
            fold_rows_key: None,
            fold_rows: Vec::new(),
            diag_rows_key: None,
            diag_rows: Vec::new(),
            filtered_pairs_galley: None,
            filtered_pairs: Vec::new(),
            fold_view_cache: None,
            style_key: 0,
            job_cache: None,
            gutter_key: None,
            gutter_text: String::new(),
            git_marks: Vec::new(),
            git_marks_rev: 0,
        }
    }
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
            completion: None,
            cache: TabCache::default(),
            diag_hash: 0,
            pending_analysis: None,
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
            completion: None,
            cache: TabCache::default(),
            diag_hash: 0,
            pending_analysis: None,
        }
    }

    pub fn display_name(&self) -> String {
        if self.dirty {
            format!("*{}", self.name)
        } else {
            self.name.clone()
        }
    }

    /// Recompute the per-character cached artifacts (`cache`) only when the
    /// content hash or the active style changed. Runs once per frame at most;
    /// no-op on idle frames. Returns the content hash so callers skip their
    /// own O(content) scan.
    ///
    /// A `pending_analysis` handed over by the loader (a heavy file opened on a
    /// worker thread) replaces the four O(content) passes below, so the first
    /// frame after such an open does no scanning at all. It is only accepted
    /// when the content hash and syntax still match the buffer.
    pub fn refresh_cache(
        &mut self,
        theme: crate::core::theme::Theme,
        editor_bg: &'static str,
        overlay: &crate::editor::guides::EditorOverlay,
        palette: &crate::core::style::Palette,
    ) -> u64 {
        let mut color_theme = theme.to_color_theme();
        color_theme.bg = editor_bg;
        let style_key =
            crate::editor::styling::style_key(&color_theme, overlay, palette, &self.syntax);
        let hash = crate::editor::diagnostics::hash_content(&self.content);
        if hash == self.diag_hash && self.cache.style_key == style_key {
            return hash;
        }
        // Take it either way: a mismatched result is dead weight, and holding
        // it would let a later edit accidentally match it.
        let offloaded = self
            .pending_analysis
            .take()
            .filter(|a| a.hash == hash && a.syntax == self.syntax);
        let (d, scan, mask, symbols, fold_opens, fold_view_cache) = match offloaded {
            Some(a) => {
                let crate::workspace::loader::Analysis {
                    diagnostics,
                    scan,
                    mask,
                    symbols,
                    fold_opens,
                    fold_view,
                    ..
                } = *a;
                let fold_view_cache = if self.folds.is_empty() {
                    // A just-opened tab has no folds, which is exactly the
                    // state the worker built the view for.
                    Some((
                        hash,
                        crate::editor::folds::fold_fp(&self.folds),
                        std::sync::Arc::new(fold_view),
                    ))
                } else {
                    None
                };
                (
                    diagnostics,
                    scan,
                    mask,
                    symbols,
                    fold_opens,
                    fold_view_cache,
                )
            }
            None => {
                let (d, scan, mask) =
                    crate::editor::diagnostics::analyze_with_scan(&self.content, &self.syntax);
                let symbols = crate::workspace::outline::extract_symbols(&self.content, &self.syntax);
                let fold_opens =
                    crate::editor::folds::foldable_opens(&self.content, &scan.brace_pairs);
                (d, scan, mask, symbols, fold_opens, None)
            }
        };
        self.cache = TabCache {
            diagnostics: d,
            symbols,
            scan,
            mask,
            fold_opens,
            fold_rows_key: None,
            fold_rows: Vec::new(),
            diag_rows_key: None,
            diag_rows: Vec::new(),
            filtered_pairs_galley: None,
            filtered_pairs: Vec::new(),
            fold_view_cache,
            style_key,
            job_cache: None,
            gutter_key: None,
            gutter_text: String::new(),
            git_marks: Vec::new(),
            git_marks_rev: 0,
        };
        self.diag_hash = hash;
        hash
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

    /// Index of the tab already showing `path`. Re-opening focuses that tab
    /// instead of reading the file a second time.
    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(path))
    }

    /// Open a file whose content *and* text passes were produced on a worker
    /// thread, so the first editor frame has nothing left to scan. Returns the
    /// index of the tab it opened (or focused, if the path was already open).
    pub fn open_loaded(
        &mut self,
        path: PathBuf,
        content: String,
        analysis: Box<crate::workspace::loader::Analysis>,
    ) -> usize {
        if let Some(i) = self.index_of(&path) {
            self.active = i;
            return i;
        }
        let syntax = analysis.syntax.clone();
        self.tabs.push(Tab::from_file(path, content, syntax));
        let idx = self.tabs.len() - 1;
        self.tabs[idx].pending_analysis = Some(analysis);
        self.active = idx;
        idx
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
            // The extension may have changed; re-pick highlight + completion.
            tab.syntax = Self::detect_syntax(&path);
        }
        Ok(path)
    }

    pub fn detect_syntax(path: &PathBuf) -> Syntax {
        match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "rs" | "rust" => Syntax::rust(),
            "py" => Syntax::python(),
            "lua" => Syntax::lua(),
            "sh" | "bash" | "zsh" => Syntax::shell(),
            "sql" => Syntax::sql(),
            "asm" | "s" => Syntax::asm(),
            "js" | "jsx" | "mjs" | "cjs" => Syntax::new("js")
                .with_comment("//")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords([
                    "async",
                    "await",
                    "break",
                    "case",
                    "catch",
                    "class",
                    "const",
                    "continue",
                    "default",
                    "delete",
                    "do",
                    "else",
                    "export",
                    "extends",
                    "finally",
                    "for",
                    "from",
                    "function",
                    "get",
                    "if",
                    "import",
                    "in",
                    "instanceof",
                    "let",
                    "new",
                    "of",
                    "return",
                    "set",
                    "static",
                    "switch",
                    "this",
                    "throw",
                    "try",
                    "typeof",
                    "var",
                    "void",
                    "while",
                    "yield",
                ])
                .with_special(["false", "null", "true", "undefined"]),
            "ts" | "tsx" | "mts" | "cts" => Syntax::new("typescript")
                .with_comment("//")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords([
                    "abstract",
                    "any",
                    "async",
                    "await",
                    "break",
                    "case",
                    "catch",
                    "class",
                    "const",
                    "continue",
                    "declare",
                    "default",
                    "delete",
                    "do",
                    "else",
                    "enum",
                    "export",
                    "extends",
                    "finally",
                    "for",
                    "from",
                    "function",
                    "get",
                    "if",
                    "implements",
                    "import",
                    "in",
                    "infer",
                    "instanceof",
                    "interface",
                    "is",
                    "keyof",
                    "let",
                    "namespace",
                    "never",
                    "new",
                    "of",
                    "readonly",
                    "return",
                    "set",
                    "static",
                    "switch",
                    "this",
                    "throw",
                    "try",
                    "type",
                    "typeof",
                    "var",
                    "void",
                    "while",
                    "yield",
                ])
                .with_types([
                    "boolean", "number", "object", "string", "symbol", "unknown", "void",
                ])
                .with_special(["false", "null", "true", "undefined"]),
            "html" | "htm" => Syntax::new("html").with_comment_multiline(["<!--", "-->"]),
            "css" => Syntax::new("css")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords([
                    "@font-face",
                    "@import",
                    "@keyframes",
                    "@media",
                    "auto",
                    "inherit",
                    "none",
                ]),
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
                    "long", "register", "return", "short", "signed", "sizeof", "static", "struct",
                    "switch", "typedef", "union", "unsigned", "void", "volatile", "while",
                ])
                .with_types([
                    "size_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t",
                    "uint32_t", "uint64_t",
                ]),
            "cc" | "cpp" | "cxx" | "hpp" => Syntax::new("cpp")
                .with_comment("//")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords([
                    "auto",
                    "break",
                    "case",
                    "catch",
                    "class",
                    "const",
                    "constexpr",
                    "continue",
                    "default",
                    "delete",
                    "do",
                    "else",
                    "enum",
                    "explicit",
                    "extern",
                    "for",
                    "friend",
                    "if",
                    "inline",
                    "namespace",
                    "new",
                    "operator",
                    "private",
                    "protected",
                    "public",
                    "return",
                    "sizeof",
                    "static",
                    "struct",
                    "switch",
                    "template",
                    "this",
                    "throw",
                    "try",
                    "typedef",
                    "typename",
                    "union",
                    "using",
                    "virtual",
                    "void",
                    "while",
                ])
                .with_types([
                    "bool", "char", "double", "float", "int", "long", "short", "size_t", "string",
                    "vector",
                ]),
            "go" => Syntax::new("go")
                .with_comment("//")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords([
                    "break",
                    "case",
                    "chan",
                    "const",
                    "continue",
                    "default",
                    "defer",
                    "else",
                    "fallthrough",
                    "for",
                    "func",
                    "go",
                    "goto",
                    "if",
                    "import",
                    "interface",
                    "map",
                    "package",
                    "range",
                    "return",
                    "select",
                    "struct",
                    "switch",
                    "type",
                    "var",
                ])
                .with_special(["false", "nil", "true"]),
            "java" => Syntax::new("java")
                .with_comment("//")
                .with_comment_multiline(["/*", "*/"])
                .with_keywords([
                    "abstract",
                    "boolean",
                    "break",
                    "byte",
                    "case",
                    "catch",
                    "char",
                    "class",
                    "const",
                    "continue",
                    "default",
                    "do",
                    "double",
                    "else",
                    "enum",
                    "extends",
                    "final",
                    "finally",
                    "float",
                    "for",
                    "goto",
                    "if",
                    "implements",
                    "import",
                    "instanceof",
                    "int",
                    "interface",
                    "long",
                    "native",
                    "new",
                    "package",
                    "private",
                    "protected",
                    "public",
                    "return",
                    "short",
                    "static",
                    "strictfp",
                    "super",
                    "switch",
                    "synchronized",
                    "this",
                    "throw",
                    "throws",
                    "transient",
                    "try",
                    "void",
                    "volatile",
                    "while",
                ])
                .with_special(["false", "null", "true"]),
            _ => Syntax::new("plain"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_syntax_rust_equals_main_and_rust_alias() {
        assert_eq!(
            TabManager::detect_syntax(&PathBuf::from("src/main.rs")).language(),
            "Rust"
        );
        assert_eq!(
            TabManager::detect_syntax(&PathBuf::from("lib.rust")).language(),
            "Rust"
        );
        assert_eq!(
            TabManager::detect_syntax(&PathBuf::from("note.txt")).language(),
            "plain"
        );
    }

    fn themed_refresh(tab: &mut Tab) -> u64 {
        tab.refresh_cache(
            crate::core::theme::Theme::GithubDark,
            "000000",
            &crate::editor::guides::EditorOverlay {
                bracket_guides: true,
                colorize_brackets: true,
            },
            &crate::core::style::Palette::dark(),
        )
    }

    const SAMPLE: &str = "fn main() {\n    let s = \"a { b\";\n    println!(\"{s}\");\n}\n";

    #[test]
    fn gutter_markers_are_taken_from_git_not_from_the_buffer() {
        // A fresh tab has no markers until a git scan hands some over; the
        // panel's revision makes the handoff happen exactly once.
        let syntax = TabManager::detect_syntax(&PathBuf::from("main.rs"));
        let mut tab = Tab::from_file(PathBuf::from("main.rs"), SAMPLE.into(), syntax);
        assert!(tab.cache.git_marks.is_empty());
        assert_eq!(tab.cache.git_marks_rev, 0);

        tab.cache.git_marks = vec![1, 4];
        tab.cache.git_marks_rev = 7;
        assert_eq!(tab.cache.git_marks, vec![1, 4]);
        assert_eq!(tab.cache.git_marks_rev, 7);
    }

    #[test]
    fn an_offthread_analysis_feeds_the_cache_and_is_consumed() {
        let syntax = TabManager::detect_syntax(&PathBuf::from("main.rs"));
        let analysis = crate::workspace::loader::analyze(SAMPLE, &syntax);
        let mut tab = Tab::from_file(PathBuf::from("main.rs"), SAMPLE.into(), syntax.clone());
        tab.pending_analysis = Some(Box::new(analysis));
        assert!(tab.pending_analysis.is_some());

        let hash = themed_refresh(&mut tab);
        assert_eq!(hash, crate::editor::diagnostics::hash_content(SAMPLE));
        // Consumed: it must not linger and be applied to some later content.
        assert!(tab.pending_analysis.is_none());

        let (d, scan, mask) = crate::editor::diagnostics::analyze_with_scan(SAMPLE, &syntax);
        assert_eq!(tab.cache.diagnostics.len(), d.len());
        assert_eq!(tab.cache.mask, mask);
        assert_eq!(tab.cache.scan.brace_pairs.len(), scan.brace_pairs.len());
        // The worker also built the fold view for the fold-less state.
        let fold_view = tab.cache.fold_view_cache.as_ref().expect("seeded");
        assert_eq!(fold_view.2.display, SAMPLE);
    }

    #[test]
    fn an_analysis_for_other_content_is_dropped_not_applied() {
        let syntax = TabManager::detect_syntax(&PathBuf::from("main.rs"));
        let stale = crate::workspace::loader::analyze(SAMPLE, &syntax);
        let edited = "fn main() {\n    let s = \"changed\";\n    println!(\"{s}\");\n}\n";
        let mut tab = Tab::from_file(PathBuf::from("main.rs"), edited.into(), syntax.clone());
        tab.pending_analysis = Some(Box::new(stale));

        let hash = themed_refresh(&mut tab);
        assert_eq!(hash, crate::editor::diagnostics::hash_content(edited));
        assert!(
            tab.pending_analysis.is_none(),
            "stale analysis must be dropped"
        );
        // The cache describes the edited buffer, not the stale one.
        let (expected, _, _) = crate::editor::diagnostics::analyze_with_scan(edited, &syntax);
        let got: Vec<_> = tab
            .cache
            .diagnostics
            .iter()
            .map(|d| (d.start, d.end, d.message.clone()))
            .collect();
        let want: Vec<_> = expected
            .iter()
            .map(|d| (d.start, d.end, d.message.clone()))
            .collect();
        assert_eq!(got, want);
        assert_eq!(
            tab.cache.fold_opens,
            crate::editor::folds::foldable_opens(edited, &tab.cache.scan.brace_pairs)
        );
        assert!(tab.cache.fold_view_cache.is_none());
    }

    #[test]
    fn an_analysis_for_another_syntax_is_dropped() {
        let rust = TabManager::detect_syntax(&PathBuf::from("main.rs"));
        let analysis = crate::workspace::loader::analyze(SAMPLE, &rust);
        // A tab whose language changed (Save As to a .txt) can't reuse it.
        let mut tab = Tab::from_file(
            PathBuf::from("main.txt"),
            SAMPLE.into(),
            TabManager::detect_syntax(&PathBuf::from("main.txt")),
        );
        tab.pending_analysis = Some(Box::new(analysis));

        themed_refresh(&mut tab);
        assert!(tab.pending_analysis.is_none());
    }

    #[test]
    fn opening_a_loaded_file_keeps_its_analysis_on_the_new_tab() {
        let mut mgr = TabManager::new();
        let path = PathBuf::from("big.rs");
        let syntax = TabManager::detect_syntax(&path);
        let analysis = crate::workspace::loader::analyze(SAMPLE, &syntax);

        let idx = mgr.open_loaded(path.clone(), SAMPLE.into(), Box::new(analysis));
        assert_eq!(idx, 0);
        assert_eq!(mgr.active, 0);
        assert!(mgr.tabs[0].pending_analysis.is_some());
        assert_eq!(mgr.tabs[0].syntax, syntax);

        // Re-opening the same path focuses the tab instead of reading again.
        let again = mgr.open_loaded(
            path,
            String::new(),
            Box::new(crate::workspace::loader::analyze("", &mgr.tabs[0].syntax.clone())),
        );
        assert_eq!(again, 0);
        assert_eq!(mgr.tabs.len(), 1);
        assert_eq!(mgr.tabs[0].content, SAMPLE, "content must not be clobbered");
    }
}
