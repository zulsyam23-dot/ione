//! Mask-aware bracket scanner.
//!
//! Scans a character buffer once, skipping masked regions (strings/comments),
//! and produces:
//! - per-character nesting depth (`u8::MAX` for non-bracket chars),
//! - all bracket pairs (including unmatched opens),
//! - `{}` brace pairs (used by folding),
//! - closing-bracket inventory and unmatched-closing diagnostics.

use super::{BracePair, BracketScan, Pair};

/// Depth value assigned to characters that are not bracket tokens.
const NO_DEPTH: u8 = u8::MAX;

/// Bracket kinds tracked by the scanner.
///
/// Order matters: indexes align with [`OPEN`] and [`CLOSE`] for O(1) matching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BracketKind {
    Paren = 0,
    Bracket = 1,
    Brace = 2,
}

impl BracketKind {
    const fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Paren,
            1 => Self::Bracket,
            _ => Self::Brace,
        }
    }

    const fn is_brace(self) -> bool {
        matches!(self, Self::Brace)
    }
}

const OPEN: [char; 3] = ['(', '[', '{'];
const CLOSE: [char; 3] = [')', ']', '}'];

#[inline]
fn is_masked(mask: &[bool], index: usize) -> bool {
    mask.get(index).copied().unwrap_or(false)
}

/// Analyze brackets in `chars`, ignoring indexes where `mask` is `true`.
///
/// # Behavior
/// - Masked chars are never treated as brackets.
/// - Matched pairs record both open and close indexes.
/// - Unmatched opening brackets are returned with `close == usize::MAX`.
/// - Unmatched closing brackets are recorded in `unmatched_closes` and get
///   depth `0` (treated as outside any currently open bracket).
/// - `closing_brackets` contains every unmasked closing bracket in source order,
///   including unmatched ones; this supports guide stacking logic.
/// - `depths[i]` is `u8::MAX` for non-bracket chars; for bracket chars it is the
///   nesting depth at that token (0-based from outermost).
pub fn analyze_brackets(chars: &[char], mask: &[bool]) -> BracketScan {
    let mut depths = vec![NO_DEPTH; chars.len()];
    let mut open_stack: Vec<(usize, BracketKind)> = Vec::new();
    let mut pairs: Vec<Pair> = Vec::with_capacity(chars.len() / 8);
    let mut brace_pairs: Vec<BracePair> = Vec::new();
    let mut closing_brackets: Vec<(usize, char)> = Vec::new();
    let mut unmatched_closes: Vec<usize> = Vec::new();

    for (index, &ch) in chars.iter().enumerate() {
        if is_masked(mask, index) {
            continue;
        }

        if let Some(kind) = open_kind(ch) {
            depths[index] = open_stack.len() as u8;
            open_stack.push((index, kind));
            continue;
        }

        if let Some(kind) = close_kind(ch) {
            closing_brackets.push((index, ch));
            match open_stack.last().copied() {
                Some((open_index, open_kind)) if open_kind == kind => {
                    open_stack.pop();
                    depths[index] = open_stack.len() as u8;
                    push_pair(&mut pairs, &mut brace_pairs, open_index, index, open_kind);
                }
                _ => {
                    // Keep rendering stable: an unmatched close does not invent nesting.
                    depths[index] = 0;
                    unmatched_closes.push(index);
                }
            }
        }
    }

    // Any remaining opens are unmatched; keep them for guides/diagnostics.
    for (open_index, kind) in open_stack {
        push_unmatched_pair(&mut pairs, &mut brace_pairs, open_index, kind);
    }

    pairs.sort_unstable_by_key(|pair| pair.open);
    brace_pairs.sort_unstable_by_key(|pair| pair.open);

    debug_assert!(depths.len() == chars.len());
    debug_assert!(pairs.iter().all(|p| p.open < chars.len()));
    debug_assert!(brace_pairs.iter().all(|p| p.open < chars.len()));

    BracketScan {
        depths,
        pairs,
        brace_pairs,
        closing_brackets,
        unmatched_closes,
    }
}

#[inline]
fn open_kind(ch: char) -> Option<BracketKind> {
    OPEN.iter()
        .position(|&open| open == ch)
        .map(BracketKind::from_index)
}

#[inline]
fn close_kind(ch: char) -> Option<BracketKind> {
    CLOSE.iter()
        .position(|&close| close == ch)
        .map(BracketKind::from_index)
}

fn push_pair(
    pairs: &mut Vec<Pair>,
    brace_pairs: &mut Vec<BracePair>,
    open: usize,
    close: usize,
    kind: BracketKind,
) {
    pairs.push(Pair { open, close });
    if kind.is_brace() {
        brace_pairs.push(BracePair { open, close });
    }
}

fn push_unmatched_pair(
    pairs: &mut Vec<Pair>,
    brace_pairs: &mut Vec<BracePair>,
    open: usize,
    kind: BracketKind,
) {
    pairs.push(Pair {
        open,
        close: usize::MAX,
    });
    if kind.is_brace() {
        brace_pairs.push(BracePair {
            open,
            close: usize::MAX,
        });
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    fn scan(content: &str) -> BracketScan {
        analyze_brackets(&content.chars().collect::<Vec<char>>(), &[])
    }

    fn scan_masked(content: &str, mask: &[bool]) -> BracketScan {
        analyze_brackets(&content.chars().collect::<Vec<char>>(), mask)
    }

    fn matched_pairs(scan: &BracketScan) -> Vec<(usize, usize)> {
        scan.pairs
            .iter()
            .filter(|p| p.close != usize::MAX)
            .map(|p| (p.open, p.close))
            .collect()
    }

    #[test]
    fn nested_depth_and_pairs() {
        let content = "fn f(a, {b}, [c]) {";
        let scan = scan(content);
        let open_paren = content.find('(').unwrap();
        let close_paren = content.find(')').unwrap();
        let open_brace_inner = content.find('{').unwrap();
        let close_brace_inner = content.find('}').unwrap();
        let open_bracket = content.find('[').unwrap();
        let close_bracket = content.find(']').unwrap();
        let open_brace_outer = content.rfind('{').unwrap();

        assert_eq!(scan.depths[open_paren], 0);
        assert_eq!(scan.depths[open_brace_inner], 1);
        assert_eq!(scan.depths[close_brace_inner], 1);
        assert_eq!(scan.depths[close_bracket], 1);
        assert_eq!(scan.depths[close_paren], 0);
        assert_eq!(scan.depths[open_brace_outer], 0);

        let pairs = matched_pairs(&scan);
        assert!(pairs.contains(&(open_paren, close_paren)));
        assert!(pairs.contains(&(open_brace_inner, close_brace_inner)));
        assert!(pairs.contains(&(open_bracket, close_bracket)));
        assert!(!pairs.iter().any(|(o, _)| *o == open_brace_outer));
    }

    #[test]
    fn mismatched_close_is_recorded() {
        let content = "(]";
        let scan = scan(content);
        assert_eq!(scan.depths, vec![0, 0]);
        assert!(matched_pairs(&scan).is_empty());
        assert_eq!(scan.unmatched_closes, vec![1]);
        assert_eq!(scan.closing_brackets, vec![(1, ']')]);
    }

    #[test]
    fn unmatched_open_is_preserved() {
        let content = "((a)";
        let scan = scan(content);
        let first_open = 0;
        let second_open = 1;
        let close = content.find(')').unwrap();
        let pairs = matched_pairs(&scan);
        assert!(pairs.contains(&(second_open, close)));
        assert!(scan
            .pairs
            .iter()
            .any(|p| p.open == first_open && p.close == usize::MAX));
    }

    #[test]
    fn masked_brackets_ignored() {
        let content = "let s = \"(a) [b]\"; sum((x))";
        let chars: Vec<char> = content.chars().collect();
        let mut mask = vec![false; chars.len()];
        let q1 = content.find('"').unwrap();
        let q2 = content.rfind('"').unwrap();
        for m in mask.iter_mut().take(q2).skip(q1 + 1) {
            *m = true;
        }

        let scan = scan_masked(content, &mask);
        // Brackets inside the string are ignored (no depth, no pair).
        assert_eq!(scan.depths[q1 + 1], NO_DEPTH);
        assert_eq!(scan.depths[q1 + 3], NO_DEPTH);
        // Code brackets after the string still pair up correctly.
        let outer_open = content.find("sum(").unwrap() + 3; // `(` in `sum((x))`
        assert_eq!(scan.depths[outer_open], 0);
        assert_eq!(scan.depths[outer_open + 1], 1);
        assert_eq!(matched_pairs(&scan).len(), 2);
    }

    #[test]
    fn brace_pairs_pair_open_close() {
        let content = "fn main() {\n    if a {\n        b();\n    }\n}\n";
        let scan = scan(content);
        let main_open = content.find('{').unwrap();
        let if_open = content.rfind("if a {").unwrap() + 5;
        let main_close = content.rfind('}').unwrap();
        let if_close = content
            .match_indices('}')
            .map(|(i, _)| i)
            .find(|&i| i != main_close)
            .unwrap();
        let pairs: Vec<(usize, usize)> =
            scan.brace_pairs.iter().map(|b| (b.open, b.close)).collect();
        assert!(pairs.contains(&(main_open, main_close)));
        assert!(pairs.contains(&(if_open, if_close)));
    }

    #[test]
    fn closing_brackets_include_unmatched() {
        let content = "a) b] c}";
        let scan = scan(content);
        assert_eq!(scan.closing_brackets, vec![(1, ')'), (4, ']'), (7, '}')]);
        assert_eq!(scan.unmatched_closes, vec![1, 4, 7]);
    }
}
