//! Bracket analysis: mask-aware scan into per-char nesting depths and the pair
//! inventory, plus cursor-aware lookup for the active-pair highlight.
//!
//! Submodules:
//! - `scanner`: the mask-aware scan producing a [`BracketScan`].
//! - `active`: the cursor's innermost enclosing pair.

pub(crate) mod active;
pub(crate) mod scanner;

pub(crate) use active::hovered_pair;
pub(crate) use scanner::analyze_brackets;

/// An opening–closing bracket pair. Unmatched opens are returned with
/// `close == usize::MAX`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pair {
    pub(crate) open: usize,
    pub(crate) close: usize,
}

/// A `{}` brace pair, used to drive folding targets. Unmatched opens have
/// `close == usize::MAX`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BracePair {
    pub(crate) open: usize,
    pub(crate) close: usize,
}

/// First-class result of a bracket scan: per-char nesting depth (used for
/// rainbow coloring), the sorted pair inventory (used for guides), the `{}`
/// pairs (used for folding), and stray closing brackets (used for diagnostics).
#[derive(Clone, Debug, Default)]
pub(crate) struct BracketScan {
    pub(crate) depths: Vec<u8>,
    pub(crate) pairs: Vec<Pair>,
    pub(crate) brace_pairs: Vec<BracePair>,
    /// Char indices of closing brackets that matched no open bracket.
    pub(crate) unmatched_closes: Vec<usize>,
}