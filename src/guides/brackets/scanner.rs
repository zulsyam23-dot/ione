//! The mask-aware scan: one pass over chars, ignoring anything masked (strings,
//! comments), producing depths, bracket pairs, and `{}` brace pairs.

use super::{BracePair, BracketScan, Pair};

const OPEN: [char; 3] = ['(', '[', '{'];
const CLOSE: [char; 3] = [')', ']', '}'];
const BRACE: usize = 2;

/// Mask-aware bracket scan: chars inside strings/comments (`mask`) are ignored,
/// so nesting depth and pairs stay correct around them. Unmatched opens are
/// returned with `close == usize::MAX`.
pub(crate) fn analyze_brackets(chars: &[char], mask: &[bool]) -> BracketScan {
    let mut depths = vec![u8::MAX; chars.len()];
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (open idx, kind)
    let mut pairs: Vec<Pair> = Vec::new();
    let mut brace_pairs: Vec<BracePair> = Vec::new();
    let mut unmatched_closes: Vec<usize> = Vec::new();
    for (ci, &ch) in chars.iter().enumerate() {
        if mask.get(ci).copied().unwrap_or(false) {
            continue;
        }
        if let Some(kind) = OPEN.iter().position(|&o| o == ch) {
            depths[ci] = stack.len() as u8;
            stack.push((ci, kind));
        } else if let Some(kind) = CLOSE.iter().position(|&c| c == ch) {
            if stack.last().is_some_and(|&(_, top)| top == kind) {
                let (oi, top_kind) = stack.pop().unwrap();
                depths[ci] = stack.len() as u8;
                pairs.push(Pair { open: oi, close: ci });
                if top_kind == BRACE {
                    brace_pairs.push(BracePair { open: oi, close: ci });
                }
            } else {
                depths[ci] = 0;
                unmatched_closes.push(ci);
            }
        }
    }
    for (oi, kind) in stack {
        pairs.push(Pair { open: oi, close: usize::MAX });
        if kind == BRACE {
            brace_pairs.push(BracePair { open: oi, close: usize::MAX });
        }
    }
    pairs.sort_by_key(|p| p.open);
    brace_pairs.sort_by_key(|p| p.open);
    BracketScan { depths, pairs, brace_pairs, unmatched_closes }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn depths_of(content: &str) -> Vec<u8> {
        let chars: Vec<char> = content.chars().collect();
        analyze_brackets(&chars, &[]).depths
    }

    fn pairs_of(content: &str) -> Vec<Pair> {
        analyze_brackets(&content.chars().collect::<Vec<char>>(), &[])
            .pairs
            .into_iter()
            .filter(|p| p.close != usize::MAX)
            .collect()
    }

    #[test]
    fn nested_depth_and_pairs() {
        let content = "fn f(a, {b}, [c]) {";
        let depths = depths_of(content);
        let open_paren = content.find('(').unwrap();
        let close_paren = content.find(')').unwrap();
        let open_brace_inner = content.find('{').unwrap();
        let close_brace_inner = content.find('}').unwrap();
        let open_bracket = content.find('[').unwrap();
        let close_bracket = content.find(']').unwrap();
        let open_brace_outer = content.rfind('{').unwrap();

        assert_eq!(depths[open_paren], 0);
        assert_eq!(depths[open_brace_inner], 1);
        assert_eq!(depths[close_brace_inner], 1);
        assert_eq!(depths[close_bracket], 1);
        assert_eq!(depths[close_paren], 0);
        assert_eq!(depths[open_brace_outer], 0);

        let paths: Vec<(usize, usize)> = pairs_of(content)
            .iter()
            .map(|p| (p.open, p.close))
            .collect();
        assert!(paths.contains(&(open_paren, close_paren)));
        assert!(paths.contains(&(open_brace_inner, close_brace_inner)));
        assert!(paths.contains(&(open_bracket, close_bracket)));
        // outer brace unmatched (no closing)
        assert!(!paths.iter().any(|(o, _)| *o == open_brace_outer));
    }

    #[test]
    fn mismatched_close_resets() {
        let content = "(]";
        assert_eq!(depths_of(content), [0, 0]);
        assert!(pairs_of(content).is_empty());
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

        let scan = &analyze_brackets(&chars, &mask);
        // Brackets inside the string are ignored (no depth, no pair).
        assert_eq!(scan.depths[q1 + 1], u8::MAX);
        assert_eq!(scan.depths[q1 + 3], u8::MAX);
        // Code brackets after the string still pair up correctly.
        let outer_open = content.find("sum(").unwrap() + 3; // `(` in `sum((x))`
        assert_eq!(scan.depths[outer_open], 0);
        assert_eq!(scan.depths[outer_open + 1], 1);
        assert_eq!(
            scan.pairs
                .iter()
                .filter(|p| p.close != usize::MAX)
                .count(),
            2
        );
    }

    #[test]
    fn brace_pairs_pair_open_close() {
        let content = "fn main() {\n    if a {\n        b();\n    }\n}\n";
        let scan = analyze_brackets(&content.chars().collect::<Vec<char>>(), &[]);
        let main_open = content.find('{').unwrap();
        let if_open = content.rfind('{').unwrap();
        let main_close = content.rfind('}').unwrap();
        let if_close = content.match_indices('}').map(|(i, _)| i).find(|&i| i != main_close).unwrap();
        let pairs: Vec<(usize, usize)> = scan.brace_pairs.iter().map(|b| (b.open, b.close)).collect();
        assert!(pairs.contains(&(main_open, main_close)));
        assert!(pairs.contains(&(if_open, if_close)));
    }
}