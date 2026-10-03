//! Source Control panel.
//!
//! The panel is split by responsibility, one concern per file:
//!
//! - [`change`] — the change model (`ChangeKind`/`GitChange`) and the
//!   `git status --porcelain` parser, both pure data.
//! - [`worker`] — every `git` subprocess and the scan that feeds the panel,
//!   all off the UI thread.
//! - [`actions`] — the mutating commands (stage, unstage, commit, discard).
//! - [`ui`] — panel chrome and the change list.
//! - [`diff`] — the selected file's diff view plus its row styling.
//!
//! This file holds the panel state itself: the public `GitPanel` type, the
//! background work bookkeeping, and the queries the rest of the app asks
//! (counts, status-bar text, explorer tints).

mod actions;
mod change;
mod diff;
mod marks;
mod ui;
mod worker;

pub use change::{ChangeKind, GitChange, parse_status};
pub use diff::DiffView;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use eframe::egui;

use crate::style::Palette;

use worker::{GitEvent, git_run, scan_workspace};

/// How often the panel re-runs `git status` while visible, so editor-driven
/// saves stay reflected without spawning git every frame.
const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(900);

pub struct GitPanel {
    pub visible: bool,
    /// Repository root resolved from the workspace (`git rev-parse
    /// --show-toplevel`); may be an ancestor of the workspace root.
    pub repo_root: Option<PathBuf>,
    pub branch: Option<String>,
    pub changes: Vec<GitChange>,
    pub selected: Option<PathBuf>,
    pub diff: Option<DiffView>,
    pub commit_msg: String,
    /// Transient inline message `(text, is_error, since)`.
    pub message: Option<(String, bool, Instant)>,
    last_run: Instant,
    /// Channel back from the background scan/diff workers.
    tx: mpsc::Sender<GitEvent>,
    rx: mpsc::Receiver<GitEvent>,
    /// Monotonic generation of the newest scan/diff issued; workers echo it so
    /// stale results are dropped.
    scan_gen: u64,
    diff_gen: u64,
    scans_in_flight: usize,
    diffs_in_flight: usize,
    /// Reset whenever scan results land; guards the `explorer_tints` cache.
    rev: u64,
    tints_cache: Option<(PathBuf, u64, HashMap<PathBuf, egui::Color32>)>,
    /// Changed lines for the file the editor is showing, as git last reported
    /// them, plus the revision they arrived with.
    marks: Option<(PathBuf, Vec<usize>)>,
    mark_rev: u64,
    /// File whose markers were last asked for. Kept so forced refreshes (stage,
    /// commit, discard) also refresh the markers instead of dropping them.
    marks_target: Option<PathBuf>,
}

impl Default for GitPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl GitPanel {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            visible: false,
            repo_root: None,
            branch: None,
            changes: Vec::new(),
            selected: None,
            diff: None,
            commit_msg: String::new(),
            message: None,
            last_run: Instant::now(),
            tx,
            rx,
            scan_gen: 0,
            diff_gen: 0,
            scans_in_flight: 0,
            diffs_in_flight: 0,
            rev: 0,
            tints_cache: None,
            marks: None,
            mark_rev: 0,
            marks_target: None,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Re-point the panel at a (possibly new) workspace root and re-scan.
    pub fn set_root(&mut self, root: Option<PathBuf>) {
        self.refresh(root.as_deref(), Instant::now(), true, None);
    }

    /// Re-read branch + status, throttled to `REFRESH_INTERVAL` unless `force`.
    /// The scan runs on a worker thread, so this never blocks the UI; results
    /// are applied on the next `poll()`. `marks_for` is the file whose changed
    /// lines the editor gutter wants; it rides along with the scan and is
    /// remembered, so later forced scans refresh the markers too.
    pub fn refresh(
        &mut self,
        root: Option<&Path>,
        now: Instant,
        force: bool,
        marks_for: Option<&Path>,
    ) -> bool {
        let elapsed = now.duration_since(self.last_run);
        if !force && elapsed < REFRESH_INTERVAL {
            return false;
        }
        self.last_run = now;
        if marks_for.is_some() {
            self.marks_target = marks_for.map(|p| p.to_path_buf());
        }
        self.spawn_scan(root, force, self.marks_target.clone());
        true
    }

    /// Changed lines git last reported for `path`; empty for anything else.
    pub fn marks_for(&self, path: &Path) -> &[usize] {
        match &self.marks {
            Some((marked, lines)) if marked == path => lines,
            _ => &[],
        }
    }

    /// Bumped whenever new markers land, so the editor copies them once.
    pub fn mark_rev(&self) -> u64 {
        self.mark_rev
    }

    /// Whether a background scan or diff is still outstanding.
    pub fn busy(&self) -> bool {
        self.scans_in_flight > 0 || self.diffs_in_flight > 0
    }

    /// Queue a status scan on a worker thread. A forced scan supersedes any
    /// in-flight one via generation numbers; throttled scans queue at most one.
    fn spawn_scan(&mut self, root: Option<&Path>, force: bool, marks_for: Option<PathBuf>) {
        if !force && self.scans_in_flight > 0 {
            return;
        }
        if self.scans_in_flight >= 2 {
            return;
        }
        self.scan_gen += 1;
        let generation = self.scan_gen;
        self.scans_in_flight += 1;
        let tx = self.tx.clone();
        let root = root.map(|p| p.to_path_buf());
        std::thread::spawn(move || {
            let outcome = scan_workspace(root.as_deref(), generation, marks_for);
            let _ = tx.send(GitEvent::Scan(outcome));
        });
    }

    /// Queue a diff for the selected change on a worker thread.
    fn spawn_diff(&mut self) {
        let (Some(repo), Some(path)) = (self.repo_root.clone(), self.selected.clone()) else {
            return;
        };
        let Some(change) = self.changes.iter().find(|c| c.path == path).cloned() else {
            return;
        };
        if self.diffs_in_flight >= 4 {
            return;
        }
        self.diff_gen += 1;
        let generation = self.diff_gen;
        self.diffs_in_flight += 1;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut args = vec!["diff"];
            if change.staged {
                args.push("--cached");
            }
            args.push("--");
            args.push(path.to_str().unwrap_or_default());
            let text = match git_run(&repo, &args) {
                Ok(o) => o,
                // Binary diffs or transient errors: no readable diff.
                Err(_) => return,
            };
            let _ = tx.send(GitEvent::Diff {
                generation,
                view: DiffView {
                    path,
                    text,
                    untracked: change.is_untracked(),
                    deleted: change.is_deleted(),
                },
            });
        });
    }

    /// Apply any finished background work to the panel. Returns `true` when a
    /// rendered field changed and the caller should request a repaint.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.rx.try_recv() {
            match event {
                GitEvent::Scan(outcome) => {
                    self.scans_in_flight = self.scans_in_flight.saturating_sub(1);
                    // A newer forced scan supersedes this one; drop stale data.
                    if outcome.generation != self.scan_gen {
                        continue;
                    }
                    self.repo_root = outcome.repo;
                    self.branch = outcome.branch;
                    if let Some(e) = outcome.error {
                        self.message = Some((e, true, Instant::now()));
                    }
                    self.apply_changes(outcome.changes);
                    if let Some(path) = outcome.marks_for {
                        self.marks = Some((path, outcome.marks));
                        self.mark_rev += 1;
                    }
                    changed = true;
                }
                GitEvent::Diff { generation, view } => {
                    self.diffs_in_flight = self.diffs_in_flight.saturating_sub(1);
                    if generation != self.diff_gen {
                        continue;
                    }
                    // A newer scan may have moved the selection; drop stale diffs.
                    if self.selected.as_deref() == Some(view.path.as_path()) {
                        self.diff = Some(view);
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    fn apply_changes(&mut self, changes: Vec<GitChange>) {
        self.changes = changes;
        self.rev += 1;
        if self
            .selected
            .as_ref()
            .is_some_and(|sel| !self.changes.iter().any(|c| c.path == *sel))
        {
            self.selected = None;
            self.diff = None;
        }
        if self.selected.is_some() {
            self.spawn_diff();
        }
    }

    /// (staged, worktree, untracked) counts.
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut staged = 0;
        let mut unstaged = 0;
        let mut untracked = 0;
        for c in &self.changes {
            if c.is_untracked() {
                untracked += 1;
            } else if c.staged {
                staged += 1;
            } else {
                unstaged += 1;
            }
        }
        (staged, unstaged, untracked)
    }

    fn has_any(&self) -> bool {
        !self.changes.is_empty()
    }

    /// Status-bar snippet `branch +N ~N ?N` when inside a repository.
    pub fn status_bar_suffix(&self) -> Option<String> {
        let repo = self.repo_root.as_ref()?;
        let _ = repo;
        let branch = self
            .branch
            .clone()
            .unwrap_or_else(|| "detached".to_string());
        let (staged, unstaged, untracked) = self.counts();
        Some(format!("{branch} +{staged} ~{unstaged} ?{untracked}"))
    }

    /// Absolute path → text color for the file explorer: changed files plus
    /// every ancestor folder down to `workspace`, so rows enclosing changes
    /// are tinted too. Rebuilt only when the change-set (`rev`) or workspace
    /// changes; idle frames get the cached map back (`&mut self` only to
    /// update the cache, no other mutation).
    pub fn explorer_tints(
        &mut self,
        workspace: &Path,
        palette: &Palette,
    ) -> HashMap<PathBuf, egui::Color32> {
        let Some(repo) = &self.repo_root else {
            self.tints_cache = None;
            return HashMap::new();
        };
        if let Some((wk, rev, cached)) = &self.tints_cache {
            if wk == workspace && *rev == self.rev {
                return cached.clone();
            }
        }
        let mut tints: HashMap<PathBuf, egui::Color32> = HashMap::new();
        for c in &self.changes {
            let abs = repo.join(&c.path);
            let color = if c.conflict {
                palette.diag_error
            } else {
                self.kind_color(palette, c.kind)
            };
            tints.insert(abs.clone(), color);

            // Tint ancestor folders (softened) down to the workspace root, so a
            // folder enclosing changes reads as dirty even when no file inside
            // it is shown directly.
            for dir in abs.ancestors().skip(1) {
                if !dir.starts_with(workspace) {
                    break;
                }
                let soft =
                    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 160);
                tints.entry(dir.to_path_buf()).or_insert(soft);
                if dir == workspace {
                    break;
                }
            }
        }
        self.tints_cache = Some((workspace.to_path_buf(), self.rev, tints.clone()));
        tints
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::style::Palette;

    #[test]
    fn explorer_tints_cover_files_and_ancestor_folders() {
        let tmp = std::env::temp_dir().join(format!("ione_git_tint_{}", std::process::id()));
        let repo = tmp.join("repo");
        let work = repo.join("src");
        let mut g = GitPanel::new();
        g.repo_root = Some(repo.clone());
        g.changes = vec![
            GitChange {
                path: PathBuf::from("src/foo.rs"),
                kind: ChangeKind::Modified,
                staged: false,
                conflict: false,
            },
            GitChange {
                path: PathBuf::from("src/bar.rs"),
                kind: ChangeKind::Untracked,
                staged: false,
                conflict: false,
            },
        ];
        let tints = g.explorer_tints(&work, &Palette::dark());
        assert!(tints.contains_key(&repo.join("src/foo.rs")));
        assert!(tints.contains_key(&repo.join("src/bar.rs")));
        assert!(tints.contains_key(&repo.join("src")));
        assert!(tints.contains_key(&work));
        assert!(!tints.contains_key(&repo.join("other.rs")));
    }
}
