//! The diff pane for the selected file, plus the unified-diff line
//! classifier and row styling behind it.

use std::path::PathBuf;

use eframe::egui::{self, RichText};

use crate::app::AppCommand;
use crate::style::Palette;

use super::{ChangeKind, GitPanel, change::kind_letter};

/// The rendered unified diff of a single selected file.
pub struct DiffView {
    pub path: PathBuf,
    pub text: String,
    pub untracked: bool,
    pub deleted: bool,
}

/// How a single unified-diff line should be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffKind {
    /// Hunk marker `@@ -x,y +a,b @@`.
    Hunk,
    /// Context (` `-prefixed or blank).
    Context,
    /// Added line (`+`).
    Add,
    /// Removed line (`-`).
    Del,
    /// File-header `--- a/…` / `+++ b/…`, rendered faint.
    Meta,
    /// `\ No newline at end of file` companion.
    NoNewline,
    /// Noise (diff --git, index, mode lines…) — hidden entirely.
    Skip,
}

/// Classify a single line of unified diff output.
fn classify_diff_line(line: &str) -> DiffKind {
    if line.starts_with("diff --git")
        || line.starts_with("index ")
        || line.starts_with("old mode")
        || line.starts_with("new mode")
        || line.starts_with("new file mode")
        || line.starts_with("deleted file mode")
        || line.starts_with("similarity index")
        || line.starts_with("rename from")
        || line.starts_with("rename to")
        || line.starts_with("Binary files")
    {
        DiffKind::Skip
    } else if line.starts_with("---") || line.starts_with("+++") {
        DiffKind::Meta
    } else if line.starts_with("@@") {
        DiffKind::Hunk
    } else if line.starts_with('+') {
        DiffKind::Add
    } else if line.starts_with('-') {
        DiffKind::Del
    } else if line.starts_with('\\') {
        DiffKind::NoNewline
    } else {
        DiffKind::Context
    }
}

/// Added/removed line counts for a diff (excluding the `+++ b/…` header).
fn diff_counts(text: &str) -> (usize, usize) {
    let mut added = 0;
    let mut removed = 0;
    for line in text.lines() {
        match classify_diff_line(line) {
            DiffKind::Add => added += 1,
            DiffKind::Del => removed += 1,
            _ => {}
        }
    }
    (added, removed)
}

/// Git's own leading `+`/`-`/space is already drawn in the sign column, so
/// painting the raw line would show every changed line's sign twice.
fn strip_diff_sign(line: &str) -> &str {
    line.strip_prefix(['+', '-', ' ']).unwrap_or(line)
}

/// Fill/text/sign styling for one diff row, tuned for the active theme.
fn diff_row_style(palette: &Palette, kind: DiffKind) -> DiffRowStyle {
    let dark = palette.text.r() > 128;
    match kind {
        DiffKind::Add => DiffRowStyle {
            fill: if dark {
                egui::Color32::from_rgba_unmultiplied(40, 132, 71, 48)
            } else {
                egui::Color32::from_rgba_unmultiplied(26, 127, 55, 28)
            },
            text: if dark {
                egui::Color32::from_rgb(115, 200, 120)
            } else {
                egui::Color32::from_rgb(26, 127, 55)
            },
            sign: "+",
        },
        DiffKind::Del => DiffRowStyle {
            fill: if dark {
                egui::Color32::from_rgba_unmultiplied(168, 43, 43, 60)
            } else {
                egui::Color32::from_rgba_unmultiplied(179, 49, 49, 26)
            },
            text: if dark {
                egui::Color32::from_rgb(230, 92, 92)
            } else {
                egui::Color32::from_rgb(179, 49, 49)
            },
            sign: "-",
        },
        DiffKind::Hunk => DiffRowStyle {
            fill: egui::Color32::from_rgba_unmultiplied(
                palette.accent.r(),
                palette.accent.g(),
                palette.accent.b(),
                40,
            ),
            text: palette.accent,
            sign: "",
        },
        DiffKind::Context => DiffRowStyle {
            fill: egui::Color32::TRANSPARENT,
            text: palette.text_muted,
            sign: "",
        },
        DiffKind::Meta | DiffKind::NoNewline => DiffRowStyle {
            fill: egui::Color32::TRANSPARENT,
            text: palette.text_muted,
            sign: "",
        },
        DiffKind::Skip => DiffRowStyle {
            fill: egui::Color32::TRANSPARENT,
            text: egui::Color32::TRANSPARENT,
            sign: "",
        },
    }
}

struct DiffRowStyle {
    fill: egui::Color32,
    text: egui::Color32,
    sign: &'static str,
}

impl GitPanel {
    pub(super) fn show_selected_diff(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        commands: &mut Vec<AppCommand>,
    ) {
        let Some(diff) = self.diff.as_ref() else {
            return;
        };
        let path = diff.path.clone();
        let text = diff.text.clone();
        let untracked = diff.untracked;
        let deleted = diff.deleted;

        if self.selected.as_deref() != Some(path.as_path()) {
            return;
        }

        let display = path.to_string_lossy().replace('\\', "/");
        ui.separator();
        ui.horizontal(|ui| {
            let kind = self.changes.iter().find(|c| c.path == path).map(|c| c.kind);
            if let Some(k) = kind {
                ui.label(
                    RichText::new(kind_letter(k))
                        .monospace()
                        .size(11.5)
                        .strong()
                        .color(self.kind_color(palette, k)),
                );
            }
            let label = RichText::new(&display)
                .monospace()
                .size(11.5)
                .color(palette.text);
            // Room for "+n −n" plus the Open File button, so the path truncates
            // instead of running under them.
            super::ui::flex_label(ui, label, 130.0, egui::Sense::hover());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(repo) = &self.repo_root
                    && !deleted
                    && !untracked
                    && ui.small_button("Open File").clicked()
                {
                    commands.push(AppCommand::OpenFilePath(repo.join(&display)));
                }
                if !untracked {
                    let (added, removed) = diff_counts(&text);
                    if added > 0 {
                        ui.label(
                            RichText::new(format!("+{added}"))
                                .monospace()
                                .size(11.5)
                                .color(self.kind_color(palette, ChangeKind::Added)),
                        );
                    }
                    if removed > 0 {
                        ui.label(
                            RichText::new(format!("−{removed}"))
                                .monospace()
                                .size(11.5)
                                .color(self.kind_color(palette, ChangeKind::Deleted)),
                        );
                    }
                }
            });
        });

        if untracked {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Untracked file — stage it (git add) to include it.")
                        .size(11.5)
                        .color(palette.text_muted),
                );
            });
        } else {
            let font = egui::FontId::monospace(12.0);
            // The scrollbar belongs at the bottom of the pane, not right under
            // the last diff line, so the scrolled area is at least as tall as
            // the pane: a two-line diff still gets its bar docked. The bar's
            // own height is subtracted so it fits instead of hanging past the
            // pane's bottom edge.
            let bar_h = ui.spacing().scroll.allocated_width();
            let viewport_h = (ui.available_height() - bar_h).max(40.0);
            egui::ScrollArea::both()
                .id_salt("git_diff_scroll")
                .auto_shrink([false, false])
                .min_scrolled_height(viewport_h)
                .show(ui, |ui| {
                    ui.add_space(2.0);
                    let row_h = ui.fonts_mut(|f| f.row_height(&font)) + 2.0;
                    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, 'm'));
                    let max_len = text.lines().map(str::len).max().unwrap_or(0);
                    ui.set_min_width((max_len as f32 * char_w).max(ui.available_width()));

                    for line in text.lines() {
                        let kind = classify_diff_line(line);
                        let style = diff_row_style(palette, kind);
                        if kind == DiffKind::Skip {
                            continue;
                        }
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width().max(0.0), row_h),
                            egui::Sense::hover(),
                        );
                        if style.fill != egui::Color32::TRANSPARENT {
                            ui.painter().rect_filled(rect, 0.0, style.fill);
                        }
                        let y = rect.center().y;
                        let x = rect.min.x + 8.0;
                        if !style.sign.is_empty() {
                            ui.painter().text(
                                egui::pos2(x, y),
                                egui::Align2::LEFT_CENTER,
                                style.sign,
                                font.clone(),
                                style.text,
                            );
                        }
                        ui.painter().text(
                            egui::pos2(x + 14.0, y),
                            egui::Align2::LEFT_CENTER,
                            strip_diff_sign(line),
                            font.clone(),
                            style.text,
                        );
                    }
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_classifier_spots_hunks_and_skips_noise() {
        let diff = "\
diff --git a/src/foo.rs b/src/foo.rs
index 123abc..456def 100644
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!(\"hi\");
-    println!(\"bye\");
 }
\\ No newline at end of file
";
        let mut kinds: Vec<DiffKind> = diff.lines().map(classify_diff_line).collect();
        assert_eq!(kinds[0], DiffKind::Skip);
        assert_eq!(kinds[1], DiffKind::Skip);
        assert_eq!(kinds[2], DiffKind::Meta);
        assert_eq!(kinds[3], DiffKind::Meta);
        assert_eq!(kinds[4], DiffKind::Hunk);
        assert_eq!(kinds[5], DiffKind::Context);
        assert_eq!(kinds[6], DiffKind::Add);
        assert_eq!(kinds[7], DiffKind::Del);
        assert_eq!(kinds[8], DiffKind::Context);
        assert_eq!(kinds[9], DiffKind::NoNewline);
    }

    #[test]
    fn the_leading_sign_is_drawn_once_in_the_sign_column() {
        assert_eq!(strip_diff_sign("+added"), "added");
        assert_eq!(strip_diff_sign("-removed"), "removed");
        assert_eq!(strip_diff_sign(" context"), "context");
        assert_eq!(strip_diff_sign("@@ -1,2 +1,2 @@"), "@@ -1,2 +1,2 @@");
        assert_eq!(strip_diff_sign("\\ No newline"), "\\ No newline");
        // Only one character is stripped, so a sign that belongs to the code
        // itself survives.
        assert_eq!(strip_diff_sign("+ +x"), " +x");
    }

    #[test]
    fn diff_counts_tally_added_and_removed() {
        let diff = "\
@@ -1,1 +1,1 @@
-old
 new
++a
++b
";
        let (added, removed) = diff_counts(diff);
        assert_eq!(added, 2);
        assert_eq!(removed, 1);
    }
}
