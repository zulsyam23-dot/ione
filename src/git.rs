use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use eframe::egui::{self, RichText};

use crate::app::AppCommand;
use crate::icons::{Icon, Icons};
use crate::style::{Palette, header_label};

/// How often the panel re-runs `git status` while visible, so editor-driven
/// saves stay reflected without spawning git every frame.
const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(900);

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

/// The rendered unified diff of a single selected file.
pub struct DiffView {
    pub path: PathBuf,
    pub text: String,
    pub untracked: bool,
    pub deleted: bool,
}

pub struct GitPanel {
    pub visible: bool,
    /// Repository root resolved from the workspace (`git rev-parse
    /// --show-toplevel`); may be an ancestor of the workspace root.
    pub repo_root: Option<PathBuf>,
    pub branch: Option<String>,
    pub changes: Vec<GitChange>,
    pub selected: Option<PathBuf>,
    pub diff: Option<DiffView>,
    pub commit_msg: String,
    /// Transient inline message `(text, is_error, since)`.
    pub message: Option<(String, bool, Instant)>,
    last_run: Instant,
}

impl Default for GitPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl GitPanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            repo_root: None,
            branch: None,
            changes: Vec::new(),
            selected: None,
            diff: None,
            commit_msg: String::new(),
            message: None,
            last_run: Instant::now(),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Repoint the panel at a (possibly new) workspace root and re-scan.
    pub fn set_root(&mut self, root: Option<PathBuf>) {
        self.refresh(root.as_deref(), Instant::now(), true);
    }

    /// Re-read branch + status, throttled to `REFRESH_INTERVAL` unless `force`.
    pub fn refresh(&mut self, root: Option<&Path>, now: Instant, force: bool) -> bool {
        let elapsed = now.duration_since(self.last_run);
        if !force && elapsed < REFRESH_INTERVAL {
            return false;
        }
        self.last_run = now;
        self.run_scan(root);
        true
    }

    fn run_scan(&mut self, root: Option<&Path>) {
        let Some(root) = root else {
            self.clear(Vec::new());
            return;
        };
        match git_toplevel(root) {
            Ok(repo) => {
                self.repo_root = Some(repo.clone());
                self.branch = git_branch(&repo);
                let parsed = git_run(&repo, &["status", "--porcelain", "-uall"])
                    .ok()
                    .map(|o| parse_status(&o))
                    .unwrap_or_default();
                self.clear(parsed);
            }
            Err(e) => {
                self.repo_root = None;
                self.message = Some((e, true, Instant::now()));
                self.clear(Vec::new());
            }
        }
    }

    fn clear(&mut self, changes: Vec<GitChange>) {
        self.changes = changes;
        if self
            .selected
            .as_ref()
            .is_some_and(|sel| !self.changes.iter().any(|c| c.path == *sel))
        {
            self.selected = None;
            self.diff = None;
        }
        self.reload_diff();
    }

    /// (staged, worktree, untracked) counts.
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut staged = 0;
        let mut unstaged = 0;
        let mut untracked = 0;
        for c in &self.changes {
            if c.is_untracked() {
                untracked += 1;
            } else if c.staged {
                staged += 1;
            } else {
                unstaged += 1;
            }
        }
        (staged, unstaged, untracked)
    }

fn has_any(&self) -> bool {
        !self.changes.is_empty()
    }

    /// Status-bar snippet `branch +N ~N ?N` when inside a repository.
    pub fn status_bar_suffix(&self) -> Option<String> {
        let repo = self.repo_root.as_ref()?;
        let _ = repo;
        let branch = self
            .branch
            .clone()
            .unwrap_or_else(|| "detached".to_string());
        let (staged, unstaged, untracked) = self.counts();
        Some(format!("{branch} +{staged} ~{unstaged} ?{untracked}"))
    }
}

impl GitPanel {
    // -- actions ------------------------------------------------------------

    fn stage_one(&mut self, path: &Path, now: Instant) {
        let Some(repo) = self.repo_root.clone() else {
            return;
        };
        match git_run(&repo, &["add", "--", &path.to_string_lossy()]) {
            Ok(_) => self.message = Some((format!("Staged {}", path.display()), false, now)),
            Err(e) => self.message = Some((e, true, now)),
        }
        self.refresh(Some(&repo), Instant::now(), true);
    }

    fn unstage_one(&mut self, path: &Path, now: Instant) {
        let Some(repo) = self.repo_root.clone() else {
            return;
        };
        match git_run(&repo, &["restore", "--staged", "--", &path.to_string_lossy()]) {
            Ok(_) => self.message = Some((format!("Unstaged {}", path.display()), false, now)),
            Err(e) => self.message = Some((e, true, now)),
        }
        self.refresh(Some(&repo), Instant::now(), true);
    }

    fn stage_all(&mut self, now: Instant) {
        let Some(repo) = self.repo_root.clone() else {
            return;
        };
        match git_run(&repo, &["add", "-A"]) {
            Ok(_) => self.message = Some(("Staged all changes".into(), false, now)),
            Err(e) => self.message = Some((e, true, now)),
        }
        self.refresh(Some(&repo), Instant::now(), true);
    }

    fn unstage_all(&mut self, now: Instant) {
        let Some(repo) = self.repo_root.clone() else {
            return;
        };
        // `git reset` (mixed) resets the index to HEAD without touching files.
        match git_run(&repo, &["reset"]) {
            Ok(_) => self.message = Some(("Unstaged all changes".into(), false, now)),
            Err(e) => self.message = Some((e, true, now)),
        }
        self.refresh(Some(&repo), Instant::now(), true);
    }

    fn commit(&mut self, now: Instant) {
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
        self.refresh(Some(&repo), Instant::now(), true);
    }

    fn discard(&mut self, path: &Path, now: Instant) {
        let Some(repo) = self.repo_root.clone() else {
            return;
        };
        // Only worktree changes are discarded; the index is never touched.
        match git_run(&repo, &["checkout", "--", &path.to_string_lossy()]) {
            Ok(_) => self.message = Some((format!("Discarded {}", path.display()), false, now)),
            Err(e) => self.message = Some((e, true, now)),
        }
        self.refresh(Some(&repo), Instant::now(), true);
    }

    fn reload_diff(&mut self) -> Option<()> {
        let repo = self.repo_root.clone()?;
        let path = self.selected.clone()?;
        let change = self.changes.iter().find(|c| c.path == path)?;

        let mut args = vec!["diff"];
        if change.staged {
            args.push("--cached");
        }
        args.push("--");
        args.push(path.to_str()?);
        let text = match git_run(&repo, &args) {
            Ok(o) => o,
            // Binary diffs or transient errors: no readable diff.
            Err(_) => return None,
        };
        self.diff = Some(DiffView {
            path,
            text,
            untracked: change.is_untracked(),
            deleted: change.is_deleted(),
        });
        Some(())
    }
}

impl GitPanel {
    // -- UI -----------------------------------------------------------------

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        icons: &mut Icons,
        palette: &Palette,
        root: Option<&PathBuf>,
        commands: &mut Vec<AppCommand>,
    ) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        self.refresh(root.map(|p| p.as_path()), now, false);
        ctx.request_repaint_after(REFRESH_INTERVAL);

        // Header: title + refresh/close.
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            ui.label(header_label(ui, "Source Control"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                if icons.image_button(ui, Icon::Close, 14.0, "Close").clicked() {
                    self.visible = false;
                }
                if icons
                    .image_button(ui, Icon::Refresh, 14.0, "Refresh status")
                    .clicked()
                {
                    self.run_scan(root.map(|p| p.as_path()));
                }
            });
        });

        self.show_message(ui, palette, now);

        if self.repo_root.is_none() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Not a git repository").color(palette.text_muted));
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Open a folder that lives inside a git work tree to see its changes here.",
                    )
                    .size(11.5)
                    .color(palette.text_muted),
                );
            });
            return;
        }

        // Branch line + action row.
        ui.horizontal(|ui| {
            let glyph = if self.branch.is_some() { "⎇" } else { "⇢" };
            ui.label(RichText::new(glyph).size(13.0).color(palette.accent));
            let branch = self
                .branch
                .clone()
                .unwrap_or_else(|| "detached HEAD".to_string());
            ui.label(RichText::new(branch).color(palette.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(self.has_any(), |ui| {
                    if ui
                        .small_button("Unstage All")
                        .on_hover_text("git reset (index only)")
                        .clicked()
                    {
                        self.unstage_all(Instant::now());
                    }
                    if ui
                        .small_button("Stage All")
                        .on_hover_text("git add -A")
                        .clicked()
                    {
                        self.stage_all(Instant::now());
                    }
                });
                let (staged, unstaged, untracked) = self.counts();
                ui.label(
                    RichText::new(format!(
                        "{}  +{staged} ~{unstaged} ?{untracked}",
                        self.changes.len()
                    ))
                    .size(11.0)
                    .color(palette.text_muted),
                );
            });
        });
        ui.add_space(6.0);

        // Commit box.
        let has_staged = self.counts().0 > 0;
        let ctrl_enter = ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter));
        let box_resp = ui.add(
            egui::TextEdit::multiline(&mut self.commit_msg)
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .hint_text("Message (Ctrl+Enter to commit)"),
        );
        if (box_resp.has_focus() && ctrl_enter) || (box_resp.lost_focus() && ctrl_enter) {
            self.commit(Instant::now());
}
        ui.horizontal(|ui| {
            let enabled = has_staged && !self.commit_msg.trim().is_empty();
            let clicked = ui
                .add_enabled(enabled, egui::Button::new("Commit"))
                .on_disabled_hover_text("Stage changes first — Commit only commits the index")
                .clicked();
            if clicked {
                self.commit(Instant::now());
            }
        });
        ui.separator();

        // Change list grouped by state.
        let mut staged: Vec<GitChange> = Vec::new();
        let mut unstaged: Vec<GitChange> = Vec::new();
        let mut untracked: Vec<GitChange> = Vec::new();
        for c in &self.changes {
            if c.is_untracked() {
                untracked.push(c.clone());
            } else if c.staged {
                staged.push(c.clone());
            } else {
                unstaged.push(c.clone());
            }
        }

        let mut open_path: Option<PathBuf> = None;
        let list_h = if self.diff.is_some() {
            ui.available_height() * 0.42
        } else {
            ui.available_height()
        };
        egui::ScrollArea::vertical()
            .id_salt("git_changes_scroll")
            .max_height(list_h)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.collapsing(format!("Staged Changes ({})", staged.len()), |ui| {
                    for c in &staged {
                        self.change_row(ui, palette, c, &mut open_path);
                    }
                });
                ui.collapsing(format!("Changes ({})", unstaged.len()), |ui| {
                    for c in &unstaged {
                        self.change_row(ui, palette, c, &mut open_path);
                    }
                });
                ui.collapsing(format!("Untracked ({})", untracked.len()), |ui| {
                    for c in &untracked {
                        self.change_row(ui, palette, c, &mut open_path);
                    }
                });
            });

        self.show_selected_diff(ui, palette, commands);

        if let Some(p) = open_path {
            commands.push(AppCommand::OpenFilePath(p));
        }
    }

    fn show_message(&mut self, ui: &mut egui::Ui, palette: &Palette, now: Instant) {
        let Some((text, is_err, since)) = self.message.take() else {
            return;
        };
let age = now.duration_since(since);
        if age < std::time::Duration::from_secs(4) {
            let color = if is_err {
                palette.diag_error
            } else {
                palette.diag_warning
            };
            ui.label(RichText::new(&text).size(11.5).color(color));
            self.message = Some((text, is_err, since));
        }
    }

    fn change_row(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        c: &GitChange,
        open_path: &mut Option<PathBuf>,
    ) {
        let display = c.path.to_string_lossy().replace('\\', "/");
        let row_height = ui.spacing().interact_size.y + 2.0;
        let row_rect = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(ui.available_width().max(0.0), row_height),
        );
        let selected = self.selected.as_deref() == Some(c.path.as_path());
        let hovered = ui.rect_contains_pointer(row_rect);
        ui.painter().rect_filled(
            row_rect,
            0.0,
            if hovered || selected {
                palette.panel_hover
            } else {
                egui::Color32::TRANSPARENT
            },
        );

        ui.horizontal(|ui| {
            ui.add_space(4.0);
            let glyph = if c.conflict { "!" } else { c.letter() };
            let color = self.glyph_color(palette, c);
            ui.label(RichText::new(glyph).monospace().size(12.0).color(color));
            let label = ui
                .add(
                    egui::Label::new(RichText::new(&display).color(palette.text))
                        .truncate()
                        .sense(egui::Sense::click()),
                );
            if label.clicked() {
                self.selected = Some(c.path.clone());
                self.reload_diff();
            }
label.context_menu(|ui| {
                if !c.is_untracked()
                    && !c.is_deleted()
                    && let Some(repo) = &self.repo_root
                    && ui.button("Open File").clicked()
                {
                    *open_path = Some(repo.join(&display));
                    ui.close();
                }
                if ui.button("Copy Path").clicked() {
                    ui.ctx().copy_text(display.clone());
                    ui.close();
                }
                ui.separator();
                if c.staged {
                    if ui.button("Unstage").clicked() {
                        self.unstage_one(&c.path, Instant::now());
                        ui.close();
                    }
} else if !c.is_untracked()
                    && ui.button("Stage").clicked()
                {
                    self.stage_one(&c.path, Instant::now());
                    ui.close();
                }
                if !c.is_untracked() && !c.staged
                    && ui.button("Discard Changes").clicked()
                {
                    self.discard(&c.path, Instant::now());
                    ui.close();
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                let act = if c.is_untracked() {
                    Some(("+", "Track this file (git add)"))
                } else if c.staged {
                    Some(("−", "Unstage"))
                } else if !c.is_deleted() {
                    Some(("+", "Stage"))
                } else {
                    None
                };
if let Some((sym, tip)) = act
                    && ui.small_button(sym).on_hover_text(tip).clicked()
                {
                    if c.staged {
                        self.unstage_one(&c.path, Instant::now());
                    } else {
                        self.stage_one(&c.path, Instant::now());
                    }
                }
            });
        });
        ui.add_space(1.0);
    }

fn show_selected_diff(
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
            let kind = self
                .changes
                .iter()
                .find(|c| c.path == path)
                .map(|c| c.kind);
            if let Some(k) = kind {
                ui.label(
                    RichText::new(kind_letter(k))
                        .monospace()
                        .size(11.5)
                        .strong()
                        .color(self.kind_color(palette, k)),
                );
            }
            ui.label(
                RichText::new(&display)
                    .monospace()
                    .size(11.5)
                    .color(palette.text),
            );
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
            egui::ScrollArea::both()
                .id_salt("git_diff_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
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
                            line,
                            font.clone(),
                            style.text,
                        );
                    }
                });
        }
    }

fn glyph_color(&self, palette: &Palette, c: &GitChange) -> egui::Color32 {
        if c.conflict {
            return palette.diag_error;
        }
        self.kind_color(palette, c.kind)
    }

    fn kind_color(&self, palette: &Palette, kind: ChangeKind) -> egui::Color32 {
        match kind {
            ChangeKind::Added | ChangeKind::Renamed => {
                if palette.text.r() > 128 {
                    egui::Color32::from_rgb(115, 200, 120)
                } else {
                    egui::Color32::from_rgb(26, 127, 55)
                }
            }
            ChangeKind::Modified => palette.diag_warning,
            ChangeKind::Deleted => palette.diag_error,
            ChangeKind::Untracked => palette.text_muted,
        }
    }
}

/// The one-letter status badge shown next to a change (`A`/`M`/`D`/`U`/`R`).
fn kind_letter(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Added => "A",
        ChangeKind::Modified => "M",
        ChangeKind::Deleted => "D",
        ChangeKind::Untracked => "U",
        ChangeKind::Renamed => "R",
    }
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

/// Run `git <args>` inside `repo` (an existing repo root). Returns stdout on
/// success, a human-readable stderr/message on failure.
fn git_run(repo: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(args);
    match cmd.output() {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
        Ok(o) => Err(stderr_or(&o)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err("git not installed".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// The repository root when `dir` (or an ancestor) is a git work tree.
fn git_toplevel(dir: &Path) -> Result<PathBuf, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(["rev-parse", "--show-toplevel"]);
    match cmd.output() {
        Ok(o) if o.status.success() => Ok(PathBuf::from(
            String::from_utf8_lossy(&o.stdout).trim(),
        )),
        Ok(o) => Err(stderr_or(&o)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err("git not installed".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

fn stderr_or(o: &std::process::Output) -> String {
    let s = String::from_utf8_lossy(&o.stderr);
    if s.trim().is_empty() {
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    } else {
        s.trim().to_string()
    }
}

fn git_branch(repo: &Path) -> Option<String> {
    let out = git_run(repo, &["branch", "--show-current"]).ok()?;
    let b = out.trim().to_string();
    if b.is_empty() {
        None
    } else {
        Some(b)
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
            ('U', _)
                | (_, 'U')
                | ('A', 'A')
                | ('D', 'D')
                | ('A', 'D')
                | ('D', 'A')
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

