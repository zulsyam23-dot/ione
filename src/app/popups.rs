use eframe::egui::{self, Align, Layout};

use crate::style::header_label;

use super::utils::load_logo;
use super::AppCommand;
use super::EditorApp;

impl EditorApp {
    pub(super) fn show_rename_window(&mut self, ctx: &egui::Context) {
        let mut state = match self.renaming.take() {
            Some(s) => s,
            None => return,
        };
        let mut submitted = false;
        let mut cancelled = false;
        egui::Window::new("Rename")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("New name:");
                let edit = ui.text_edit_singleline(&mut state.input);
                if !edit.has_focus() {
                    edit.request_focus();
                }
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        submitted = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                });
                if enter {
                    submitted = true;
                }
                if esc {
                    cancelled = true;
                }
            });
        if submitted {
            let p = state.path.clone();
            let n = state.input.clone();
            self.apply_rename(&p, &n);
            self.renaming = None;
        } else if cancelled {
            self.renaming = None;
        } else {
            self.renaming = Some(state);
        }
    }

    pub(super) fn show_about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }

        let p = self.palette;
        egui::Window::new("About ione")
            .collapsible(false)
            .resizable(true)
            .default_width(440.0)
            .show(ctx, |ui| {
                if self.logo.is_none() {
                    self.logo = Some(load_logo(ctx));
                }

                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    // Header: logo + name + tagline.
                    ui.horizontal(|ui| {
                        if let Some(tex) = &self.logo {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::hover());
                            ui.painter().image(
                                tex.id(),
                                rect,
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                        }
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("ione")
                                    .size(22.0)
                                    .strong()
                                    .color(p.text),
                            );
                            ui.label(
                                egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                                    .size(12.0)
                                    .color(p.text_muted),
                            );
                        });
                    });
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(
                            "ione is a lightweight, multifunctional code editor with an \
                             integrated terminal, built for a fast and distraction-free workflow.",
                        )
                        .size(13.0)
                        .color(p.text_muted),
                    );

                    // Features.
                    ui.add_space(14.0);
                    ui.label(header_label(ui, "Features"));
                    ui.add_space(4.0);
                    for feat in [
                        "Multi-tab code editor with syntax highlighting",
                        "Integrated multi-session PowerShell terminal",
                        "File explorer with expand / collapse & context menu",
                        "Find & Replace with live results",
                        "Code folding with chevron icons",
                        "Multi-select editing (Ctrl+D)",
                        "Error/warning squiggle decorations",
                        "Bracket pair guides & rainbow brackets",
                        "Dark & light themes (pure black / white based)",
                        "Open Folder flow with recent-file navigation",
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("•").color(p.accent));
                            ui.label(egui::RichText::new(feat).size(12.5).color(p.text));
                        });
                    }

                    // Shortcuts.
                    ui.add_space(14.0);
                    ui.label(header_label(ui, "Shortcuts"));
                    ui.add_space(4.0);
                    let shortcuts: [(&str, &str); 13] = [
                        ("New File", "Ctrl+N"),
                        ("Open File", "Ctrl+O"),
                        ("Open Folder", "Ctrl+K Ctrl+O"),
                        ("Save", "Ctrl+S"),
                        ("Save As", "Ctrl+Shift+S"),
                        ("Close Tab", "Ctrl+W"),
                        ("Find & Replace", "Ctrl+H"),
                        ("Toggle File Explorer", "Ctrl+L"),
                        ("Toggle Terminal", "Ctrl+`"),
                        ("Select Next Word", "Ctrl+D"),
                        ("Fold Block", "Ctrl+Shift+["),
                        ("Unfold Block", "Ctrl+Shift+]"),
                        ("Tab Cycle", "Ctrl+Tab / Ctrl+Shift+Tab"),
                    ];
                    for (name, sc) in shortcuts {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(name).size(12.5).color(p.text));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(egui::RichText::new(sc).color(p.text_muted));
                            });
                        });
                    }

                    // Tech stack.
                    ui.add_space(14.0);
                    ui.label(header_label(ui, "Tech Stack"));
                    ui.add_space(4.0);
                    for tech in [
                        "Rust (edition 2024)",
                        "egui / eframe 0.36",
                        "egui_code_editor",
                        "portable-pty + vt100 (terminal)",
                        "resvg + egui_extras (icons)",
                    ] {
                        ui.label(egui::RichText::new(format!("• {tech}")).size(12.5).color(p.text));
                    }

                    // Footer.
                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "© ione — Rust code editor. Built with ♥ and egui."
                        ))
                        .size(11.5)
                        .color(p.text_muted),
                    );
                    ui.add_space(4.0);
                });

                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    if ui
                        .add_sized([100.0, 28.0], egui::Button::new("Close"))
                        .clicked()
                    {
                        self.show_about = false;
                    }
                });
            });
    }

    pub(super) fn show_theme_confirm(&mut self, ctx: &egui::Context, commands: &mut Vec<AppCommand>) {
        let Some(theme) = self.pending_theme else {
            return;
        };
        let mut cancel = false;
        let mut confirm = false;
        let p = self.palette;
        egui::Window::new(format!("{} (beta)", theme.name()))
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.colored_label(
                    p.accent,
                    format!("Tema {} masih dalam tahap beta.", theme.name()),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "Sebagian warna/aksen dapat berubah di versi berikutnya. Tetap memakainya?",
                    )
                    .size(12.5)
                    .color(p.text_muted),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Pakai tetap").clicked() {
                        confirm = true;
                    }
                    if ui.button("Batal").clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            commands.push(AppCommand::SetTheme(theme));
        } else if cancel {
            self.pending_theme = None;
        }
    }

    pub(super) fn show_font_msg(&mut self, ctx: &egui::Context) {
        let Some((msg, t0)) = self.font_msg.clone() else {
            return;
        };
        let dur = 3.5_f64;
        let left = dur - t0.elapsed().as_secs_f64();
        if left <= 0.0 {
            self.font_msg = None;
            return;
        }
        let alpha = ((left / dur).clamp(0.0, 1.0) as f32) * 255.0;
        egui::Area::new(egui::Id::new("font_msg"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(self.palette.panel_active)
                    .stroke(egui::Stroke::new(1.0, self.palette.border))
                    .corner_radius(0)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(msg)
                                .color(self.palette.text.gamma_multiply(alpha)),
                        );
                    });
            });
        ctx.request_repaint();
    }
}