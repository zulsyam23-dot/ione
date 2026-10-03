//! Off-thread file opening: reading a file and running its text-derived passes
//! (lex mask, bracket scan, diagnostics, symbols, fold view) on a worker thread,
//! so the UI thread keeps painting and stays responsive instead of freezing
//! while the file is opened. The loading GIF goes up for the whole wait. Same
//! worker pattern as the Git panel (`git.rs`): an `mpsc` channel, a monotonic
//! generation so a superseded load is dropped, and a `poll()` that applies
//! whatever finished since the last frame.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use egui_code_editor::Syntax;

use crate::diagnostics::Diagnostic;
use crate::editor::folds::FoldView;
use crate::guides::BracketScan;
use crate::outline::Symbol;

/// Upper bound on concurrent load workers; past it a new request is refused
/// instead of stacking another full read + analysis.
const MAX_CONCURRENT_LOADS: usize = 4;

/// Every per-character artifact derived from one buffer, computed once so the
/// first editor frame has nothing left to scan. All fields are plain owned data
/// (`Vec`/`String`/`usize`), so this crosses the thread boundary cheaply and
/// without touching any egui type.
pub struct Analysis {
    /// `diagnostics::hash_content` of the exact content this was built from —
    /// the key `Tab::refresh_cache` matches against.
    pub hash: u64,
    /// The syntax it was lexed with; Save As can change a tab's language
    /// without changing its content, so the two must agree before reuse.
    pub syntax: Syntax,
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: Vec<Symbol>,
    pub scan: BracketScan,
    /// String/comment char mask (shared with auto-close and bracket logic).
    pub mask: Vec<bool>,
    pub fold_opens: Vec<usize>,
    /// Fold view for an empty fold set — the state a freshly opened tab is in.
    /// Rebuilt on the UI thread as soon as the user folds something.
    pub fold_view: FoldView,
}

/// A finished (or failed) load, tagged with the generation that requested it.
pub enum LoadEvent {
    Done {
        generation: u64,
        path: PathBuf,
        content: String,
        analysis: Box<Analysis>,
    },
    Failed {
        generation: u64,
        path: PathBuf,
        error: String,
    },
}

/// A finished load, ready to become a tab: content plus the text passes that
/// were computed alongside it.
pub struct LoadedFile {
    pub path: PathBuf,
    pub content: String,
    pub analysis: Box<Analysis>,
}

/// A load in flight: what the overlay caption shows while the worker runs.
pub struct PendingLoad {
    pub path: PathBuf,
    pub bytes: u64,
}

pub struct FileLoader {
    tx: mpsc::Sender<LoadEvent>,
    rx: mpsc::Receiver<LoadEvent>,
    /// Monotonic generation of the newest requested load; workers echo it so
    /// stale results are dropped.
    generation: u64,
    in_flight: usize,
    pub pending: Option<PendingLoad>,
}

impl Default for FileLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl FileLoader {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            generation: 0,
            in_flight: 0,
            pending: None,
        }
    }

    /// Whether a background read/analysis is still outstanding.
    pub fn busy(&self) -> bool {
        self.in_flight > 0
    }

    /// Queue `path` for reading + analysis on a worker thread. Supersedes any
    /// load already in flight: the older result is dropped when it lands.
    /// Returns `false` when too many are already running, in which case this
    /// request was not queued at all.
    pub fn request(&mut self, path: PathBuf, bytes: u64) -> bool {
        // Each request is a full read plus a full O(n) analysis; cap the burst
        // so a click-spam can't pile up threads, but leave room for the
        // deliberate "open these two at once" case.
        if self.in_flight >= MAX_CONCURRENT_LOADS {
            return false;
        }
        self.generation += 1;
        let generation = self.generation;
        self.in_flight += 1;
        self.pending = Some(PendingLoad {
            path: path.clone(),
            bytes,
        });
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let event = match read_and_analyze(&path) {
                Ok((content, analysis)) => LoadEvent::Done {
                    generation,
                    path,
                    content,
                    analysis: Box::new(analysis),
                },
                Err(error) => LoadEvent::Failed {
                    generation,
                    path,
                    error,
                },
            };
            let _ = tx.send(event);
        });
        true
    }

    /// Apply whatever finished since the last call, newest generation only.
    /// Returns the successful loads to open as tabs, plus error messages to
    /// surface in the status bar.
    pub fn poll(&mut self) -> (Vec<LoadedFile>, Vec<String>) {
        let mut loaded = Vec::new();
        let mut errors = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            self.in_flight = self.in_flight.saturating_sub(1);
            match event {
                LoadEvent::Done {
                    generation,
                    path,
                    content,
                    analysis,
                } => {
                    // A newer load superseded this one; drop the stale data.
                    if generation != self.generation {
                        continue;
                    }
                    loaded.push(LoadedFile {
                        path,
                        content,
                        analysis,
                    });
                }
                LoadEvent::Failed {
                    generation,
                    path,
                    error,
                } => {
                    if generation != self.generation {
                        continue;
                    }
                    errors.push(format!("failed to open {}: {error}", path.display()));
                }
            }
        }
        if !self.busy() {
            self.pending = None;
        }
        (loaded, errors)
    }
}

/// Human-readable byte count for the overlay caption (`12.4 MB`).
pub fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// File name for the overlay caption, falling back to the full path.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// Size on disk, `0` when it can't be read (the read itself reports the error).
pub fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// The worker body: read the file, then every O(n) pass the first editor frame
/// would otherwise run on the UI thread.
fn read_and_analyze(path: &Path) -> Result<(String, Analysis), String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let syntax = crate::tabs::TabManager::detect_syntax(&path.to_path_buf());
    // Borrow, then move the content out — no clone of a possibly huge buffer.
    let analysis = analyze(&content, &syntax);
    Ok((content, analysis))
}

/// All text-derived artifacts for `content`. Pure and thread-safe; the UI path
/// in `Tab::refresh_cache` falls back to exactly these passes when no
/// off-thread result is available.
pub fn analyze(content: &str, syntax: &Syntax) -> Analysis {
    let (diagnostics, scan, mask) = crate::diagnostics::analyze_with_scan(content, syntax);
    let symbols = crate::outline::extract_symbols(content, syntax);
    let fold_opens = crate::editor::folds::foldable_opens(content, &scan.brace_pairs);
    Analysis {
        hash: crate::diagnostics::hash_content(content),
        syntax: syntax.clone(),
        diagnostics,
        symbols,
        scan,
        mask,
        fold_opens,
        fold_view: crate::editor::folds::build_fold_view(content, &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> String {
        "fn main() {\n    let s = \"a { b\";\n    println!(\"{s}\");\n}\n".to_string()
    }

    #[test]
    fn analyze_matches_the_inline_passes() {
        let content = sample();
        let syntax = Syntax::rust();
        let a = analyze(&content, &syntax);

        let (d, scan, mask) = crate::diagnostics::analyze_with_scan(&content, &syntax);
        let symbols = crate::outline::extract_symbols(&content, &syntax);
        let fold_opens = crate::editor::folds::foldable_opens(&content, &scan.brace_pairs);

        assert_eq!(a.hash, crate::diagnostics::hash_content(&content));
        assert_eq!(a.diagnostics.len(), d.len());
        assert_eq!(a.symbols.len(), symbols.len());
        assert_eq!(a.scan.brace_pairs.len(), scan.brace_pairs.len());
        assert_eq!(a.mask, mask);
        assert_eq!(a.fold_opens, fold_opens);
        // A freshly opened tab has no folds, so the display text is the file.
        assert_eq!(a.fold_view.display, content);
        assert_eq!(a.syntax, syntax);
    }

    #[test]
    fn load_applies_to_the_matching_generation() {
        let mut loader = FileLoader::new();
        let dir = std::env::temp_dir().join("ione-loader-test");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("sample.rs");
        std::fs::write(&path, sample()).expect("write");

        loader.request(path.clone(), 42);
        assert!(loader.busy());
        assert_eq!(loader.pending.as_ref().map(|p| p.bytes), Some(42));

        // `poll` returns before the worker finishes; keep pumping until it lands.
        let mut loaded = Vec::new();
        for _ in 0..200 {
            let (done, errors) = loader.poll();
            assert!(errors.is_empty(), "unexpected errors: {errors:?}");
            if !done.is_empty() {
                loaded = done;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(loaded.len(), 1, "worker never reported back");
        let file = loaded.into_iter().next().expect("checked");
        assert_eq!(file.path, path);
        assert_eq!(file.content, sample());
        assert_eq!(
            file.analysis.hash,
            crate::diagnostics::hash_content(&file.content)
        );
        assert!(!loader.busy());
        assert!(loader.pending.is_none());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_newer_request_supersedes_the_older_one() {
        let mut loader = FileLoader::new();
        let dir = std::env::temp_dir().join("ione-loader-test-stale");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let old = dir.join("old.rs");
        let new = dir.join("new.rs");
        std::fs::write(&old, "fn old() {}\n").expect("write old");
        std::fs::write(&new, "fn new() {}\n").expect("write new");

        loader.request(old.clone(), 1);
        loader.request(new.clone(), 2);

        let mut loaded = Vec::new();
        let mut errors = Vec::new();
        for _ in 0..200 {
            let (done, errs) = loader.poll();
            loaded.extend(done);
            errors.extend(errs);
            if !loader.busy() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(loaded.len(), 1, "the superseded load must be dropped");
        assert_eq!(loaded[0].path, new);

        let _ = std::fs::remove_file(&old);
        let _ = std::fs::remove_file(&new);
    }

    #[test]
    fn a_missing_file_reports_an_error() {
        let mut loader = FileLoader::new();
        let missing = std::env::temp_dir().join("ione-loader-test-missing.rs");
        let _ = std::fs::remove_file(&missing);
        loader.request(missing.clone(), 0);

        let mut errors = Vec::new();
        for _ in 0..200 {
            let (loaded, errs) = loader.poll();
            assert!(loaded.is_empty(), "a failed read must not open a tab");
            errors.extend(errs);
            if !loader.busy() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(errors.len(), 1, "expected one error, got {errors:?}");
        assert!(errors[0].contains("failed to open"), "{errors:?}");
    }

    #[test]
    fn human_size_is_readable_at_every_scale() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2 KB");
        assert_eq!(human_size(12_400_000), "11.8 MB");
    }
}
