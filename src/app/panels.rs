use std::path::PathBuf;

use eframe::egui::{self, Align, Align2, Color32, Layout, RichText};

use crate::icons::Icon;
use crate::menu;
use crate::style::{frame, header_label, Palette};
use crate::tabs::TabManager;
use crate::diagnostics::Severity;

use super::EditorApp;
use super::utils::{drag_vertical_splitter, load_logo, HANDLE};
use super::AppCommand;

impl EditorApp {
    pub(super) fn show_title_bar(&mut self, root_ui: &mut egui::Ui, commands: &mut Vec<AppCommand>) {
        egui::Panel::top("title_bar")
            .frame(frame(self.palette.bg, Color32::TRANSPARENT, 0, 3))
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    egui::menu::MenuBar::new().ui(ui, |ui| {
                        menu::show_menu_bar(ui, commands, &self.editor_font, &self.guides());
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(8.0);
                        if self
                            .icons
                            .image_button(ui, Icon::Terminal, 14.0, "Toggle Terminal (Ctrl+`)")
                            .clicked()
                        {
                            commands.push(AppCommand::ToggleTerminal);
                        }
                    });
                });
            });
    }

    pub(super) fn show_sidebar(&mut self, commands: &mut Vec<AppCommand>, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(header_label(ui, "Explorer"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self
                    .icons
                    .image_button(ui, Icon::Refresh, 14.0, "Refresh")
                    .clicked()
                {
                    commands.push(AppCommand::RefreshFileTree);
                }
            });
        });
        ui.add_space(8.0);

        let total_h = ui.available_height();
        let total_w = ui.available_width();
        let tree_h = ((total_h - HANDLE) * (1.0 - self.outline_frac)).max(40.0);
        let outline_h = (total_h - tree_h - HANDLE).max(40.0);

        ui.allocate_ui(egui::vec2(total_w, tree_h), |ui| {
            self.file_tree.show(ui, &mut self.icons, &self.palette, commands);
        });

        drag_vertical_splitter(ui, self.palette.border, &mut self.outline_frac, total_h);

        ui.allocate_ui(egui::vec2(total_w, outline_h), |ui| {
            let (content, syntax) = match self.tabs.active_tab() {
                Some(tab) => (tab.content.clone(), tab.syntax.clone()),
                None => (String::new(), egui_code_editor::Syntax::new("plain")),
            };
            if !content.is_empty() {
                if let Some(line) = self.outline.show(ui, &self.palette, &content, &syntax) {
                    if let Some(tab) = self.tabs.active_tab_mut() {
                        tab.goto_line = Some(line);
                    }
                }
            }
        });
    }

    pub(super) fn show_empty_state(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Center the block strictly within the area that remains after the
        // terminal/status panels, so it can never spill over them.
        let avail = ui.available_rect_before_wrap();

        if self.logo.is_none() {
            self.logo = Some(load_logo(ctx));
        }

        let logo_sz = 72.0_f32;
        let gap = 16.0_f32;
        let title_h = 40.0_f32;
        let sub_h = 24.0_f32;
        let total = logo_sz + gap + title_h + 10.0 + sub_h;
        let cx = avail.center().x;
        let mut y = avail.center().y - total / 2.0;

        if let Some(tex) = &self.logo {
            let rect = egui::Rect::from_center_size(
                egui::pos2(cx, y + logo_sz / 2.0),
                egui::vec2(logo_sz, logo_sz),
            );
            ui.painter().image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        y += logo_sz + gap;

        let title_rc = egui::Rect::from_center_size(
            egui::pos2(cx, y + title_h / 2.0),
            egui::vec2(300.0, title_h),
        );
        ui.painter().text(
            title_rc.center(),
            Align2::CENTER_CENTER,
            "ione",
            egui::FontId::proportional(36.0),
            self.palette.text,
        );
        y += title_h + 10.0;

        let sub_rc = egui::Rect::from_center_size(
            egui::pos2(cx, y + sub_h / 2.0),
            egui::vec2(500.0, sub_h),
        );
        ui.painter().text(
            sub_rc.center(),
            Align2::CENTER_CENTER,
            "Open a file or create a new one to get started.",
            egui::FontId::proportional(15.0),
            self.palette.text_muted,
        );
    }

    pub(super) fn show_status_bar(root_ui: &mut egui::Ui, tabs: &TabManager, branch: &str) {
        let p = Palette::dark();
        egui::Panel::bottom("status_bar")
            .frame(frame(p.bg, p.border, 0, 5))
            .show(root_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(header_label(ui, &format!(" {}", branch)));
                    ui.separator();
                    if let Some(tab) = tabs.active_tab() {
                        let lines = tab.content.chars().filter(|&c| c == '\n').count() + 1;
                        ui.label(
                            RichText::new(format!("Ln {}, Col {}  ({lines} lines)", tab.cursor_line + 1, tab.cursor_col + 1))
                                .color(p.text),
                        );
                        ui.separator();
                        ui.label(RichText::new("UTF-8").color(p.text_muted));
                        let errs = tab
                            .diagnostics
                            .iter()
                            .filter(|d| d.severity == Severity::Error)
                            .count();
                        let warns = tab
                            .diagnostics
                            .iter()
                            .filter(|d| d.severity == Severity::Warning)
                            .count();
                        if errs > 0 || warns > 0 {
                            ui.separator();
                            ui.label(
                                RichText::new(format!("{errs} errors · {warns} warnings"))
                                    .color(p.text),
                            );
                        }
                    } else {
                        ui.label(RichText::new("No file open").color(p.text_muted));
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(tab) = tabs.active_tab() {
                            let lang = if tab.path.is_none() {
                                "Plain Text".to_string()
                            } else {
                                tab.syntax.language().to_string()
                            };
                            ui.label(RichText::new(lang).color(p.text));
                            ui.separator();
                        }
                        ui.separator();
                        ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).color(p.text_muted));
                    });
                });
            });
    }

    pub(super) fn show_tab_bar(tabs: &mut TabManager, ui: &mut egui::Ui, p: Palette) {
        if tabs.is_empty() {
            return;
        }

        let mut switch_to: Option<usize> = None;
        let mut close_idx: Option<usize> = None;

        egui::Panel::top("tab_bar")
            .frame(frame(ui.visuals().extreme_bg_color, Color32::TRANSPARENT, 0, 5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    for (i, tab) in tabs.tabs.iter().enumerate() {
                        let active = i == tabs.active;
                        let text = tab.display_name();
                        let label = if tab.dirty {
                            format!("● {text}")
                        } else {
                            text
                        };
                        let color = if active { p.text } else { p.text_muted };
                        let fill = if active { p.panel_active } else { Color32::TRANSPARENT };
                        let tab_frame = egui::Frame::NONE
                            .fill(fill)
                            .corner_radius(0.0);
                        let response = tab_frame.show(ui, |ui| {
                            ui.add(
                                egui::Button::new(RichText::new(&label).color(color)),
                            )
                        })
                        .inner;

                        if response.clicked() {
                            switch_to = Some(i);
                        }
                        response.context_menu(|ui| {
                            if ui.button("Close").clicked() {
                                close_idx = Some(i);
                                ui.close();
                            }
                        });
                        if response.middle_clicked() {
                            close_idx = Some(i);
                        }
                        ui.add_space(2.0);
                    }
                });
            });

        if let Some(idx) = switch_to {
            tabs.set_active(idx);
        }
        if let Some(idx) = close_idx {
            tabs.close(idx);
        }
    }

    pub(super) fn show_breadcrumbs(&self, ui: &mut egui::Ui, commands: &mut Vec<AppCommand>) {
        let Some(tab) = self.tabs.active_tab() else {
            return;
        };
        let Some(path) = &tab.path else {
            return;
        };
        egui::Panel::top("breadcrumbs")
            .frame(frame(self.palette.panel, Color32::TRANSPARENT, 0, 8))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut cumulative = PathBuf::new();
                    for comp in path.components() {
                        use std::path::Component;
                        match comp {
                            Component::RootDir => {
                                cumulative.push(std::path::MAIN_SEPARATOR.to_string());
                            }
                            Component::CurDir | Component::ParentDir => continue,
                            Component::Normal(os) => {
                                cumulative.push(os);
                                let seg = cumulative.clone();
                                let label = os.to_string_lossy().to_string();
                                let is_file = cumulative == *path;
                                let color = if is_file {
                                    self.palette.text_muted
                                } else {
                                    self.palette.text
                                };
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new(&label).color(color))
                                            .frame(false),
                                    )
                                    .on_hover_text(seg.to_string_lossy().to_string())
                                    .clicked()
                                {
                                    commands.push(AppCommand::SetRoot(seg));
                                }
                                ui.label(RichText::new("/").color(self.palette.text_muted));
                            }
                            Component::Prefix(_) => {}
                        }
                    }
                });
            });
    }

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