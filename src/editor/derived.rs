//! Derived, per-frame state computed from the tab's content + fold set.

use crate::workspace::tabs::Tab;
use super::folds::FoldView;

/// Drop folds whose brace target no longer exists (content or fold set
/// changed underneath them).
pub(crate) fn prune_stale_folds(tab: &mut Tab) {
    if tab.folds.is_empty() {
        return;
    }
    let valid: std::collections::HashSet<usize> = tab
        .cache
        .scan
        .brace_pairs
        .iter()
        .filter(|b| b.close != usize::MAX)
        .map(|b| b.open)
        .collect();
    tab.folds.retain(|f| valid.contains(&f.open));
}

/// Gutter diagnostic rows: the display row of each diagnostic, sorted and
/// deduped so an Error wins over a Warning on the same row.
pub(crate) fn compute_diag_rows(
    view: &FoldView,
    diagnostics: &[crate::editor::diagnostics::Diagnostic],
) -> Vec<(usize, crate::editor::diagnostics::Severity)> {
    let line_of_char = |ci: usize| {
        view.real_line_starts
            .partition_point(|&s| s <= ci)
            .saturating_sub(1)
    };
    let mut rows = Vec::new();
    for d in diagnostics {
        let real_line = line_of_char(d.start);
        if let Some(row) = view.display_row_of(real_line) {
            rows.push((row, d.severity));
        }
    }
    use crate::editor::diagnostics::Severity;
    rows.sort_by_key(|&(row, severity)| (row, severity == Severity::Warning));
    rows.dedup_by_key(|&mut (row, _)| row);
    rows
}

