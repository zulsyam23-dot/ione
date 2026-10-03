//! The change model behind the panel and the `git status --porcelain` parser.
//!
//! Pure data: no UI, no subprocesses, no panel state.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// Staged new file (`A` in the index).
    Added,
    /// Modified in the worktree and/or the index (`M`).
    Modified,
    /// Deleted in the worktree and/or the index (`D`).
    Deleted,
    /// Untracked (`??`).
    Untracked,
    /// Renamed (`R`, staged).
    Renamed,
}

#[derive(Debug, Clone)]
pub struct GitChange {
    /// Repository-relative path (git-style `/` separators).
    pub path: PathBuf,
    pub kind: ChangeKind,
    /// `true` when the change is already in the index.
    pub staged: bool,
    /// Merge conflict state (`UU`, `AU`, …) — staging marks it resolved.
    pub conflict: bool,
}

impl GitChange {
    pub fn letter(&self) -> &'static str {
        kind_letter(self.kind)
    }

    pub fn is_untracked(&self) -> bool {
        matches!(self.kind, ChangeKind::Untracked)
    }

    pub fn is_deleted(&self) -> bool {
        matches!(self.kind, ChangeKind::Deleted)
    }
}

/// The one-letter status badge shown next to a change (`A`/`M`/`D`/`U`/`R`).
pub(super) fn kind_letter(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Added => "A",
        ChangeKind::Modified => "M",
        ChangeKind::Deleted => "D",
        ChangeKind::Untracked => "U",
        ChangeKind::Renamed => "R",
    }
}

/// Parse `git status --porcelain -uall` output into changes.
pub fn parse_status(out: &str) -> Vec<GitChange> {
    let mut changes = Vec::new();
    for line in out.lines() {
        let bytes = line.as_bytes();
        if bytes.len() < 4 {
            continue;
        }
        let x = bytes[0] as char;
        let y = bytes[1] as char;
        let rest = &line[3..];

        if x == '?' && y == '?' {
            changes.push(GitChange {
                path: PathBuf::from(rest),
                kind: ChangeKind::Untracked,
                staged: false,
                conflict: false,
            });
            continue;
        }

        let (kind, path) = if x == 'R' || y == 'R' {
            // `R  old -> new` — porcelain v1 splits on ` -> `.
            let new_path = rest
                .rsplit_once(" -> ")
                .map(|(_, new)| new.trim())
                .unwrap_or(rest);
            (ChangeKind::Renamed, PathBuf::from(new_path))
        } else {
            let kind = match (x, y) {
                ('A', _) => ChangeKind::Added,
                ('D', _) => ChangeKind::Deleted,
                (_, 'D') => ChangeKind::Deleted,
                (_, _) => ChangeKind::Modified,
            };
            (kind, PathBuf::from(rest))
        };

        // Porcelain v1 unmerged (conflict) state: both columns non-blank and no
        // longer a plain stage+worktree (`MM`); letters among A/D/U.
        let unmerged = matches!(
            (x, y),
            ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') | ('A', 'D') | ('D', 'A')
        );

        changes.push(GitChange {
            path,
            kind,
            staged: x != ' ' && x != '?',
            conflict: unmerged,
        });
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_worktree_and_staged_lines() {
        let out = " M src/foo.rs\nM  src/bar.rs\nA  src/new.rs\n D gone.rs\nD  old.rs\n";
        let cs = parse_status(out);
        assert_eq!(cs.len(), 5);
        assert_eq!(cs[0].kind, ChangeKind::Modified);
        assert!(!cs[0].staged);
        assert_eq!(cs[1].kind, ChangeKind::Modified);
        assert!(cs[1].staged);
        assert_eq!(cs[2].kind, ChangeKind::Added);
        assert!(cs[2].staged);
        assert_eq!(cs[3].kind, ChangeKind::Deleted);
        assert!(!cs[3].staged);
        assert_eq!(cs[4].kind, ChangeKind::Deleted);
        assert!(cs[4].staged);
    }

    #[test]
    fn parse_untracked_and_conflict() {
        let out = "?? readme.md\nUU conflict.rs\nAA both-new.rs\n";
        let cs = parse_status(out);
        assert_eq!(cs[0].kind, ChangeKind::Untracked);
        assert!(!cs[0].staged);
        assert_eq!(cs[1].kind, ChangeKind::Modified);
        assert!(cs[1].conflict);
        assert_eq!(cs[2].kind, ChangeKind::Added);
        assert!(cs[2].conflict);
    }

    #[test]
    fn parse_rename_takes_new_path() {
        let out = "R  old.rs -> new.rs\n";
        let cs = parse_status(out);
        assert_eq!(cs[0].kind, ChangeKind::Renamed);
        assert!(cs[0].staged);
        assert_eq!(cs[0].path.to_string_lossy(), "new.rs");
    }
}
