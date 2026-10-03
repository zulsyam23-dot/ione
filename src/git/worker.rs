//! The `git` subprocess layer and the background status scan.
//!
//! Nothing here touches the UI: `scan_workspace` is called on a worker thread
//! by [`super::GitPanel::spawn_scan`], and the mutating commands in
//! [`super::actions`] borrow [`git_run`].

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{GitChange, parse_status};

/// Background-worker result, delivered to `GitPanel::poll`.
pub(super) enum GitEvent {
    Scan(ScanOutcome),
    Diff {
        generation: u64,
        view: super::DiffView,
    },
}

/// Status-scan snapshot computed off the UI thread. `generation` lets `poll`
/// drop results from superseded scans (a user action can force a newer one).
pub(super) struct ScanOutcome {
    pub(super) generation: u64,
    pub(super) repo: Option<PathBuf>,
    pub(super) branch: Option<String>,
    pub(super) changes: Vec<GitChange>,
    pub(super) error: Option<String>,
    /// Changed lines for the file the editor is showing, and the path they
    /// belong to (the editor may have switched tabs since the scan was queued).
    pub(super) marks_for: Option<PathBuf>,
    pub(super) marks: Vec<usize>,
}

/// Run `git <args>` inside `repo` (an existing repo root). Returns stdout on
/// success, a human-readable stderr/message on failure.
pub(super) fn git_run(repo: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(args);
    match cmd.output() {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
        Ok(o) => Err(stderr_or(&o)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err("git not installed".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// The repository root when `dir` (or an ancestor) is a git work tree.
fn git_toplevel(dir: &Path) -> Result<PathBuf, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"]);
    match cmd.output() {
        Ok(o) if o.status.success() => Ok(PathBuf::from(String::from_utf8_lossy(&o.stdout).trim())),
        Ok(o) => Err(stderr_or(&o)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err("git not installed".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

fn stderr_or(o: &std::process::Output) -> String {
    let s = String::from_utf8_lossy(&o.stderr);
    if s.trim().is_empty() {
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    } else {
        s.trim().to_string()
    }
}

fn git_branch(repo: &Path) -> Option<String> {
    let out = git_run(repo, &["branch", "--show-current"]).ok()?;
    let b = out.trim().to_string();
    if b.is_empty() { None } else { Some(b) }
}

/// Read-only git snapshot for a workspace root, run on a worker thread.
/// `marks_for` is the file the editor gutter wants markers for; its diff rides
/// along with the status scan instead of spawning another git process.
pub(super) fn scan_workspace(
    root: Option<&Path>,
    generation: u64,
    marks_for: Option<PathBuf>,
) -> ScanOutcome {
    let Some(root) = root else {
        return ScanOutcome {
            generation,
            repo: None,
            branch: None,
            changes: Vec::new(),
            error: None,
            marks_for,
            marks: Vec::new(),
        };
    };
    match git_toplevel(root) {
        Ok(repo) => {
            let branch = git_branch(&repo);
            let changes = git_run(&repo, &["status", "--porcelain", "-uall"])
                .ok()
                .map(|o| parse_status(&o))
                .unwrap_or_default();
            let marks = marks_for
                .as_deref()
                .map(|path| super::marks::changed_lines(&repo, path))
                .unwrap_or_default();
            ScanOutcome {
                generation,
                repo: Some(repo),
                branch,
                changes,
                error: None,
                marks_for,
                marks,
            }
        }
        Err(e) => ScanOutcome {
            generation,
            repo: None,
            branch: None,
            changes: Vec::new(),
            error: Some(e),
            marks_for,
            marks: Vec::new(),
        },
    }
}
