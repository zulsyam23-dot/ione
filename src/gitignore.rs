//! `.gitignore` support for the file explorer.
//!
//! The explorer walks the tree with `std::fs`, so without this ignored paths
//! (`target/`, `.idea/`, `node_modules/`, build output) would sit in the list
//! like real sources. Shell out to `git check-ignore` on every scan would block
//! the UI thread, so the common pattern syntax is matched here instead:
//! comments, negation, directory-only rules, anchoring, `*`, `?`, `**` and
//! character classes.
//!
//! Rules are per directory: a `.gitignore` applies to its own subtree only, and
//! a deeper file overrides a shallower one - the same precedence git uses.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Ignore rules collected per directory level while the explorer walks down.
pub struct Ignores {
    /// Outermost first; the last entry is the directory currently being listed.
    levels: Vec<Level>,
    /// Parsed files by directory, so a refresh re-walks without re-reading.
    cache: HashMap<PathBuf, Option<Vec<Rule>>>,
}

struct Level {
    dir: PathBuf,
    rules: Vec<Rule>,
}

#[derive(Clone)]
struct Rule {
    negated: bool,
    /// `build/` matches directories only, not a file named `build`.
    dir_only: bool,
    /// `/build` or `src/build` match from the `.gitignore`'s own directory
    /// instead of any level below it.
    anchored: bool,
    pattern: String,
}

impl Default for Ignores {
    fn default() -> Self {
        Self::new()
    }
}

impl Ignores {
    pub fn new() -> Self {
        Self {
            levels: Vec::new(),
            cache: HashMap::new(),
        }
    }

    /// Descend into `dir`, picking up its `.gitignore` if it has one.
    pub fn enter(&mut self, dir: &Path) {
        if !self.cache.contains_key(dir) {
            let rules = std::fs::read_to_string(dir.join(".gitignore"))
                .ok()
                .map(|text| parse(&text));
            self.cache.insert(dir.to_path_buf(), rules);
        }
        if let Some(rules) = self.cache.get(dir).cloned().flatten() {
            self.levels.push(Level {
                dir: dir.to_path_buf(),
                rules,
            });
        }
    }

    /// Walk back out to `dir`, dropping the levels deeper than it. Directories
    /// without a `.gitignore` push no level, so the stack can be shallower than
    /// the walk - only levels *inside* `dir` are dropped, never an ancestor's.
    pub fn leave(&mut self, dir: &Path) {
        while self
            .levels
            .last()
            .is_some_and(|level| level.dir != dir && level.dir.starts_with(dir))
        {
            self.levels.pop();
        }
    }

    /// Whether the explorer should hide `path`.
    pub fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        // Innermost file first, and within it the last matching line wins.
        for level in self.levels.iter().rev() {
            // A `.gitignore` only governs its own subtree, so a pattern from a
            // deeper directory never reaches back out to its parent's files.
            let Ok(rel) = path.strip_prefix(&level.dir) else {
                continue;
            };
            for rule in level.rules.iter().rev() {
                if !rule.matches(rel, &name, is_dir) {
                    continue;
                }
                return !rule.negated;
            }
        }
        false
    }
}

impl Rule {
    fn matches(&self, rel: &Path, name: &str, is_dir: bool) -> bool {
        if self.dir_only && !is_dir {
            return false;
        }
        if self.anchored {
            let rel = rel.to_string_lossy().replace('\\', "/");
            glob(&self.pattern, &rel)
        } else {
            // No slash in the pattern: it applies at any depth below, so only
            // the file name has to match.
            glob(&self.pattern, name)
        }
    }
}

/// Parse the lines of a `.gitignore`.
fn parse(text: &str) -> Vec<Rule> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<Rule> {
    let line = line.trim_end_matches([' ', '\t', '\r']);
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (mut negated, mut rest) = match line.strip_prefix('!') {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    // An escaped marker is a literal character, not syntax.
    if let Some(tail) = rest.strip_prefix("\\#") {
        rest = tail;
        negated = false;
    } else if let Some(tail) = rest.strip_prefix("\\!") {
        rest = tail;
        negated = false;
    }
    let dir_only = rest.ends_with('/');
    let rest = rest.trim_end_matches('/');
    // A slash anywhere but at the end anchors the pattern to this directory.
    let anchored = rest.contains('/');
    let pattern = rest.strip_prefix('/').unwrap_or(rest).to_string();
    if pattern.is_empty() {
        return None;
    }
    Some(Rule {
        negated,
        dir_only,
        anchored,
        pattern,
    })
}

/// Glob match with git's flavour: `*` and `?` stop at `/`, `**` crosses it.
fn glob(pattern: &str, text: &str) -> bool {
    match_bytes(pattern.as_bytes(), text.as_bytes())
}

fn match_bytes(pattern: &[u8], text: &[u8]) -> bool {
    let Some((first, rest)) = pattern.split_first() else {
        return text.is_empty();
    };
    match *first {
        b'*' => {
            let doubles = rest.first() == Some(&b'*');
            let rest = if doubles { &rest[1..] } else { rest };
            // `**/` stands for zero or more directories, so the slash behind
            // it may be consumed too: `**/logs` matches a plain `logs`.
            let rest = match rest.strip_prefix(b"/".as_slice()) {
                Some(after) if doubles => after,
                _ => rest,
            };
            for skip in 0..=text.len() {
                if !doubles && text[..skip].contains(&b'/') {
                    break;
                }
                if match_bytes(rest, &text[skip..]) {
                    return true;
                }
            }
            false
        }
        b'?' => !text.is_empty() && text[0] != b'/' && match_bytes(rest, &text[1..]),
        b'[' => match_class(rest, text)
            .is_some_and(|(pattern, taken)| match_bytes(&rest[pattern..], &text[taken..])),
        b'\\' if rest.first() == Some(&b'*') => match_bytes(rest, text),
        c => !text.is_empty() && text[0] == c && match_bytes(rest, &text[1..]),
    }
}

/// A `[...]` class at the head of `rest`: `(pattern length including ']',
/// characters taken from the text)`.
fn match_class(rest: &[u8], text: &[u8]) -> Option<(usize, usize)> {
    let end = rest.iter().position(|b| *b == b']')?;
    let (negated, items) = match rest.first() {
        Some(b'!') | Some(b'^') => (true, &rest[1..end]),
        _ => (false, &rest[..end]),
    };
    let first = *text.first()?;
    let mut hit = false;
    let mut i = 0;
    while i < items.len() {
        if i + 2 < items.len() && items[i + 1] == b'-' {
            if first >= items[i] && first <= items[i + 2] {
                hit = true;
            }
            i += 3;
        } else {
            if first == items[i] {
                hit = true;
            }
            i += 1;
        }
    }
    // A class stands for exactly one character, whatever its range.
    (hit != negated).then_some((end + 1, 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ignored(patterns: &str, rel: &str, is_dir: bool) -> bool {
        let mut stack = Ignores::new();
        let root = PathBuf::from("/repo");
        let rules = parse(patterns);
        stack.levels.push(Level {
            dir: root.clone(),
            rules,
        });
        stack.is_ignored(&root.join(rel), is_dir)
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let rules = parse("# a comment\n\n   \ntarget\n");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].pattern, "target");
    }

    #[test]
    fn a_plain_name_matches_at_any_depth() {
        assert!(ignored("target\n", "target", true));
        assert!(ignored("target\n", "src/target", true));
        assert!(ignored("target\n", "src/deep/target", true));
        assert!(!ignored("target\n", "src/targeted", true));
    }

    #[test]
    fn a_slash_anchors_the_pattern_to_its_directory() {
        assert!(ignored("/build\n", "build", true));
        assert!(!ignored("/build\n", "src/build", true));
        assert!(ignored("src/build\n", "src/build", true));
        assert!(!ignored("src/build\n", "other/src/build", true));
    }

    #[test]
    fn directory_only_rules_skip_files() {
        assert!(ignored("build/\n", "build", true));
        assert!(!ignored("build/\n", "build", false));
    }

    #[test]
    fn the_last_matching_rule_wins() {
        assert!(!ignored("*.log\n!keep.log\n", "keep.log", false));
        assert!(ignored("*.log\n!keep.log\n", "drop.log", false));
        // Order matters: the negation has to come last.
        assert!(ignored("!keep.log\n*.log\n", "keep.log", false));
    }

    #[test]
    fn stars_span_one_segment_and_double_stars_span_many() {
        assert!(ignored("src/*.rs\n", "src/main.rs", false));
        assert!(!ignored("src/*.rs\n", "src/nested/main.rs", false));
        assert!(ignored("src/**/gen.rs\n", "src/a/b/gen.rs", false));
        assert!(ignored("**/logs\n", "logs", true));
        assert!(ignored("**/logs\n", "a/b/logs", true));
        // `a/**` covers everything inside `a`, but not `a` itself.
        assert!(ignored("a/**\n", "a/b/c", false));
        assert!(!ignored("a/**\n", "a", true));
    }

    #[test]
    fn character_classes_and_single_wildcards() {
        assert!(ignored("file?.txt\n", "file1.txt", false));
        assert!(!ignored("file?.txt\n", "file12.txt", false));
        assert!(ignored("v[0-9].txt\n", "v7.txt", false));
        assert!(!ignored("v[0-9].txt\n", "vx.txt", false));
        assert!(ignored("v[!0-9].txt\n", "vx.txt", false));
    }

    #[test]
    fn escaped_markers_are_literal() {
        assert!(!ignored("\\#notes\n", "#notes", false));
        assert!(ignored("\\#notes\n", "notes", false));
        assert!(!ignored("\\!bang\n", "!bang", false));
    }

    #[test]
    fn deeper_files_override_shallower_ones() {
        let root = PathBuf::from("/repo");
        let mut stack = Ignores::new();
        stack.levels.push(Level {
            dir: root.clone(),
            rules: parse("*.log\n"),
        });
        assert!(stack.is_ignored(Path::new("/repo/a.log"), false));

        let sub = root.join("sub");
        stack.levels.push(Level {
            dir: sub.clone(),
            rules: parse("!a.log\n"),
        });
        assert!(!stack.is_ignored(Path::new("/repo/sub/a.log"), false));
        // The outer file still applies to its own subtree elsewhere.
        assert!(stack.is_ignored(Path::new("/repo/a.log"), false));

        stack.leave(&root);
        assert_eq!(stack.levels.len(), 1);
        assert!(stack.is_ignored(Path::new("/repo/sub/a.log"), false));
    }

    #[test]
    fn a_subtree_without_a_gitignore_keeps_the_parent_rules() {
        let root = PathBuf::from("/repo");
        let mut stack = Ignores::new();
        stack.levels.push(Level {
            dir: root.clone(),
            rules: parse("target/\n"),
        });
        // `enter` on a directory with no `.gitignore` pushes nothing, so the
        // stack is shallower than the walk; leaving must not drop the root.
        let plain = root.join("plain");
        stack.enter(&plain);
        assert_eq!(stack.levels.len(), 1);
        assert!(stack.is_ignored(&root.join("target"), true));
        stack.leave(&plain);
        assert_eq!(stack.levels.len(), 1);
        assert!(stack.is_ignored(&root.join("target"), true));
    }

    #[test]
    fn no_gitignore_means_nothing_is_ignored() {
        assert!(!ignored("", "target", true));
        assert!(!ignored("# only comments\n", "target", true));
    }
}
