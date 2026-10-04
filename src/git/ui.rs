//! Panel rendering: header, branch line, commit box, and the change list.
//!
//! The diff pane lives in [`super::diff`]; the row/branch colors used by both
//! are defined here as [`GitPanel::kind_color`] and [`GitPanel::glyph_color`].

use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::{self, RichText};

use crate::app::AppCommand;
use crate::core::icons::{Icon, Icons};
use crate::core::style::{Palette, header_label};

use super::{ChangeKind, GitChange, GitPanel, REFRESH_INTERVAL};

/// Width kept free on the right of a row for its action cluster. egui lays out
/// left to right, so a flexible label that grabs everything squeezes the
/// right-aligned widgets into the same pixels - the text then draws on top of
/// them. Reserving the space keeps both readable.
const ACTIONS_W: f32 = 34.0;

/// A label that yields `reserve` pixels on the right instead of eating them.
pub(super) fn flex_label(
    ui: &mut egui::Ui,
    text: impl Into<egui::WidgetText>,
    reserve: f32,
    sense: egui::Sense,
) -> egui::Response {
    let height = ui.spacing().interact_size.y;
    let width = (ui.available_width() - reserve).max(24.0);
    ui.add_sized(
        [width, height],
        egui::Label::new(text).truncate().sense(sense),
    )
}

impl GitPanel {
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
        // Apply finished background results first so this frame shows fresh data.
        if self.poll() {
            ctx.request_repaint();
        }
        self.refresh(root.map(|p| p.as_path()), now, false, None);
        // While a scan/diff is in flight, poll frequently; otherwise keep the
        // throttled cadence so external edits still surface.
        ctx.request_repaint_after(if self.busy() {
            std::time::Duration::from_millis(120)
        } else {
            REFRESH_INTERVAL
        });

        // Header: title + refresh/close.
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            ui.label(header_label(ui, "Source Control"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                if icons.image_button(ui, Icon::Close, 16.0, "Close").clicked() {
                    self.visible = false;
                }
                if icons
                    .image_button(ui, Icon::Refresh, 16.0, "Refresh status")
                    .clicked()
                {
                    self.spawn_scan(root.map(|p| p.as_path()), true, self.marks_target.clone());
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

        // Branch line, then the actions on their own row. The counts and the two
        // buttons need more width than a branch name can spare in this panel,
        // and egui gives no way to reserve a widget's width up front - sharing
        // one row meant the gray counts drew on top of the branch text.
        ui.horizontal(|ui| {
            let glyph = if self.branch.is_some() { "⎇" } else { "⇢" };
            ui.label(RichText::new(glyph).size(13.0).color(palette.accent));
            let branch = self
                .branch
                .clone()
                .unwrap_or_else(|| "detached HEAD".to_string());
            flex_label(
                ui,
                RichText::new(branch).color(palette.text),
                0.0,
                egui::Sense::hover(),
            );
        });
        ui.horizontal(|ui| {
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
                    RichText::new(format!("+{staged} ~{unstaged} ?{untracked}"))
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
            let label = flex_label(
                ui,
                RichText::new(&display).color(palette.text),
                ACTIONS_W,
                egui::Sense::click(),
            );
            if label.clicked() {
                self.selected = Some(c.path.clone());
                self.spawn_diff();
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
                } else if !c.is_untracked() && ui.button("Stage").clicked() {
                    self.stage_one(&c.path, Instant::now());
                    ui.close();
                }
                if !c.is_untracked() && !c.staged && ui.button("Discard Changes").clicked() {
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

    pub(super) fn glyph_color(&self, palette: &Palette, c: &GitChange) -> egui::Color32 {
        if c.conflict {
            return palette.diag_error;
        }
        self.kind_color(palette, c.kind)
    }

    pub(super) fn kind_color(&self, palette: &Palette, kind: ChangeKind) -> egui::Color32 {
        match kind {
            ChangeKind::Added | ChangeKind::Renamed => palette.added,
            ChangeKind::Modified => palette.diag_warning,
            ChangeKind::Deleted => palette.diag_error,
            ChangeKind::Untracked => palette.text_muted,
        }
    }
}
