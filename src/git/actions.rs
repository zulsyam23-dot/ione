//! Mutating git commands run by the panel's buttons and row menus.
//!
//! Each one shells out through [`git_run`], leaves a transient message on the
//! panel, then forces a refresh so the next scan reflects reality.

use std::path::Path;
use std::time::Instant;

use super::{GitPanel, worker::git_run};

impl GitPanel {
    pub(super) fn stage_one(&mut self, path: &Path, now: Instant) {
        let Some(repo) = self.repo_root.clone() else {
            return;
        };
        match git_run(&repo, &["add", "--", &path.to_string_lossy()]) {
            Ok(_) => self.message = Some((format!("Staged {}", path.display()), false, now)),
            Err(e) => self.message = Some((e, true, now)),
        }
        self.refresh(Some(&repo), Instant::now(), true, None);
    }

    pub(super) fn unstage_one(&mut self, path: &Path, now: Instant) {
        let Some(repo) = self.repo_root.clone() else {
            return;
        };
        match git_run(
            &repo,
            &["restore", "--staged", "--", &path.to_string_lossy()],
        ) {
            Ok(_) => self.message = Some((format!("Unstaged {}", path.display()), false, now)),
            Err(e) => self.message = Some((e, true, now)),
        }
        self.refresh(Some(&repo), Instant::now(), true, None);
    }

    pub(super) fn stage_all(&mut self, now: Instant) {
        let Some(repo) = self.repo_root.clone() else {
            return;
        };
        match git_run(&repo, &["add", "-A"]) {
            Ok(_) => self.message = Some(("Staged all changes".into(), false, now)),
            Err(e) => self.message = Some((e, true, now)),
        }
        self.refresh(Some(&repo), Instant::now(), true, None);
    }

    pub(super) fn unstage_all(&mut self, now: Instant) {
        let Some(repo) = self.repo_root.clone() else {
            return;
        };
        // `git reset` (mixed) resets the index to HEAD without touching files.
        match git_run(&repo, &["reset"]) {
            Ok(_) => self.message = Some(("Unstaged all changes".into(), false, now)),
            Err(e) => self.message = Some((e, true, now)),
        }
        self.refresh(Some(&repo), Instant::now(), true, None);
    }

    pub(super) fn commit(&mut self, now: Instant) {
        let Some(repo) = self.repo_root.clone() else {
            return;
        };
        let msg = self.commit_msg.trim();
        if msg.is_empty() {
            return;
        }
        match git_run(&repo, &["commit", "-m", msg]) {
            Ok(_) => {
                self.commit_msg.clear();
                self.message = Some(("Committed".into(), false, now));
            }
            Err(e) => self.message = Some((e, true, now)),
        }
        self.refresh(Some(&repo), Instant::now(), true, None);
    }

    pub(super) fn discard(&mut self, path: &Path, now: Instant) {
        let Some(repo) = self.repo_root.clone() else {
            return;
        };
        // Only worktree changes are discarded; the index is never touched.
        match git_run(&repo, &["checkout", "--", &path.to_string_lossy()]) {
            Ok(_) => self.message = Some((format!("Discarded {}", path.display()), false, now)),
            Err(e) => self.message = Some((e, true, now)),
        }
        self.refresh(Some(&repo), Instant::now(), true, None);
    }
}
