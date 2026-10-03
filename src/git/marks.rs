//! Changed-line markers for the editor gutter, straight from `git diff`.
//!
//! The Source Control panel already knows what git sees, so rather than diffing
//! the buffer a second time we ask git for the hunk headers of the file being
//! edited (`-U0`, no context) and paint the lines it reports. The marker
//! therefore tracks the same change set as the panel: it appears while the file
//! is edited and stays after saving, until the change is committed or reverted.

use std::path::Path;

use super::worker::git_run;

/// 0-based line indices git reports as added or modified, ascending.
///
/// Only hunk headers are read, so the diff body is ignored; with `-U0` the
/// headers alone are the full answer.
pub(crate) fn parse_hunks(diff: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for line in diff.lines() {
        let Some(header) = line.strip_prefix("@@") else {
            continue;
        };
        let Some((old, _)) = header.split_once("@@") else {
            continue;
        };
        let Some(new) = old.split_whitespace().nth(1) else {
            continue;
        };
        let Some((start, count)) = parse_range(new) else {
            continue;
        };
        // `count == 0` is a pure deletion: those lines are gone from the file
        // and have no row left to mark.
        out.extend(start..start + count);
    }
    out
}

/// `"+12,3"` or `"+12"` as `(0-based start, count)`.
fn parse_range(spec: &str) -> Option<(usize, usize)> {
    let spec = spec.strip_prefix('+')?;
    let (start, count) = match spec.split_once(',') {
        Some((start, count)) => (start, count),
        None => (spec, "1"),
    };
    let start: usize = start.parse().ok()?;
    let count: usize = count.parse().ok()?;
    // Hunk starts are 1-based line numbers; 0 is a zero-length old range.
    Some((start.saturating_sub(1), count))
}

/// Lines git reports as changed for `path` against `HEAD`, or empty when git
/// cannot answer: no repository, no commit yet, untracked file, git missing.
pub(super) fn changed_lines(repo: &Path, path: &Path) -> Vec<usize> {
    let path = path.to_str().unwrap_or_default();
    match git_run(repo, &["diff", "HEAD", "-U0", "--", path]) {
        Ok(diff) => parse_hunks(&diff),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_changed_line_is_one_marker() {
        assert_eq!(parse_hunks("@@ -2,1 +2,1 @@\n-a\n+b\n"), vec![1]);
    }

    #[test]
    fn a_hunk_marks_its_whole_new_range() {
        assert_eq!(parse_hunks("@@ -10,2 +10,4 @@\n"), vec![9, 10, 11, 12]);
    }

    #[test]
    fn a_hunk_without_a_count_is_a_single_line() {
        assert_eq!(parse_hunks("@@ -1 +7 @@\n"), vec![6]);
    }

    #[test]
    fn an_inserted_block_is_marked() {
        // Two old lines replaced by five: the five new rows are the change.
        assert_eq!(parse_hunks("@@ -3,2 +3,5 @@\n"), vec![2, 3, 4, 5, 6]);
    }

    #[test]
    fn a_pure_deletion_marks_nothing() {
        assert!(parse_hunks("@@ -4,3 +3,0 @@\n").is_empty());
    }

    #[test]
    fn several_hunks_come_back_in_order() {
        let diff = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n@@ -8,1 +9,2 @@\n@@ -20,0 +22,1 @@\n";
        assert_eq!(parse_hunks(diff), vec![0, 1, 8, 9, 21]);
    }

    #[test]
    fn text_without_hunk_headers_yields_no_markers() {
        assert!(parse_hunks("").is_empty());
        assert!(parse_hunks("no changes\n").is_empty());
        assert!(parse_hunks("@@ nonsense @@\n").is_empty());
    }

    #[test]
    fn ranges_of_zero_lines_are_dropped() {
        assert_eq!(parse_range("+0,3"), Some((0, 3)));
        assert_eq!(parse_range("+5,0"), Some((4, 0)));
        assert_eq!(parse_range("-5,3"), None);
    }
}
