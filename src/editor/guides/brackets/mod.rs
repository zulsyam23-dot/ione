//! Bracket analysis: mask-aware scan into per-char nesting depths and the pair
//! inventory, plus cursor-aware lookup for the active-pair highlight.
//!
//! Submodules:
//! - `scanner`: the mask-aware scan producing a [`BracketScan`].
//! - `active`: the cursor's innermost enclosing pair.

pub mod active;
pub mod scanner;

pub use scanner::analyze_brackets;

/// An opening–closing bracket pair. Unmatched opens are returned with
/// `close == usize::MAX`.
#[derive(Clone, Copy, Debug)]
pub struct Pair {
    pub open: usize,
    pub close: usize,
}

/// A `{}` brace pair, used to drive folding targets. Unmatched opens have
/// `close == usize::MAX`.
#[derive(Clone, Copy, Debug)]
pub struct BracePair {
    pub open: usize,
    pub close: usize,
}

/// First-class result of a bracket scan: per-char nesting depth (used for
/// rainbow coloring), the sorted pair inventory (used for guides), the `{}`
/// pairs (used for folding), and stray closing brackets (used for diagnostics).
#[derive(Clone, Debug, Default)]
pub struct BracketScan {
    pub depths: Vec<u8>,
    pub pairs: Vec<Pair>,
    pub brace_pairs: Vec<BracePair>,
    /// Closing bracket character indices in source order.
    pub closing_brackets: Vec<(usize, char)>,
    /// Char indices of closing brackets that matched no open bracket.
    pub unmatched_closes: Vec<usize>,
}
