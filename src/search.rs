use std::path::PathBuf;

use eframe::egui;

use crate::app::AppCommand;
use crate::icons::{Icon, Icons};

#[derive(Clone)]
pub struct FileResult {
    pub path: PathBuf,
    pub line_no: u32,
    pub line_text: String,
    pub col: usize,
}

pub struct SearchPanel {
    pub query: String,
    pub replacement: String,
    pub show_replace: bool,
    pub case_sensitive: bool,
    pub search_files: bool,
    pub total_matches: usize,
    pub current_match: usize,
    pub file_results: Vec<FileResult>,
    pub visible: bool,
}

impl SearchPanel {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            replacement: String::new(),
            show_replace: false,
            case_sensitive: false,
            search_files: false,
            total_matches: 0,
            current_match: 0,
            file_results: Vec::new(),
            visible: false,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        icons: &mut Icons,
        content: &str,
        root: Option<&PathBuf>,
        commands: &mut Vec<AppCommand>,
    ) {
        if !self.visible {
            return;
        }

        egui::Panel::bottom("search_panel").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Find:");
                let response = ui.text_edit_singleline(&mut self.query);
                if response.changed() {
                    if self.search_files {
                        self.run_file_search(root, &self.query.clone(), self.case_sensitive);
                    } else {
                        self.count_matches(content);
                        if !self.query.is_empty() {
                            commands.push(AppCommand::FindNext(self.query.clone()));
                        }
                    }
                }

                let cs_changed = ui.checkbox(&mut self.case_sensitive, "Aa").changed() && !self.query.is_empty();
                if cs_changed {
                    if self.search_files {
                        self.run_file_search(root, &self.query.clone(), self.case_sensitive);
                    } else {
                        self.count_matches(content);
                    }
                }

                if !self.search_files
                    && ui
                        .add(egui::Button::new("In Files").selected(false))
                        .on_hover_text("Search all files in the workspace")
                        .clicked()
                {
                    self.search_files = true;
                    self.run_file_search(root, &self.query.clone(), self.case_sensitive);
                } else if self.search_files
                    && ui
                        .add(egui::Button::new("In File").selected(true))
                        .clicked()
                {
                    self.search_files = false;
                    self.count_matches(content);
                }

                if self.search_files {
                    ui.label(format!("{} results", self.file_results.len()));
                } else if self.total_matches > 0 {
                    ui.label(format!("{}/{}", self.current_match, self.total_matches));
                }

                if icons
                    .image_button(ui, Icon::ArrowLeft, 14.0, "Previous match")
                    .clicked()
                {
                    commands.push(AppCommand::FindPrev(self.query.clone()));
                    self.prev_match();
                }
                if icons
                    .image_button(ui, Icon::ArrowRight, 14.0, "Next match")
                    .clicked()
                {
                    commands.push(AppCommand::FindNext(self.query.clone()));
                    self.next_match();
                }

                ui.separator();

                if ui
                    .add(egui::Button::new("Replace").selected(self.show_replace))
                    .clicked()
                {
                    self.show_replace = !self.show_replace;
                }

                if icons
                    .image_button(ui, Icon::Close, 14.0, "Close")
                    .clicked()
                {
                    self.visible = false;
                }
            });

            if self.search_files {
                self.show_file_results(ui, commands);
            } else if self.show_replace {
                ui.horizontal(|ui| {
                    ui.label("Replace:");
                    ui.text_edit_singleline(&mut self.replacement);
                    if ui.button("Replace").clicked() {
                        if !self.query.is_empty() {
                            commands.push(AppCommand::Replace(
                                self.query.clone(),
                                self.replacement.clone(),
                            ));
                            self.count_matches(content);
                        }
                    }
                    if ui.button("Replace All").clicked() {
                        if !self.query.is_empty() {
                            commands.push(AppCommand::ReplaceAll(
                                self.query.clone(),
                                self.replacement.clone(),
                            ));
                            self.count_matches(content);
                        }
                    }
                });
            }
            ui.add_space(6.0);
        });
    }

    fn show_file_results(&mut self, ui: &mut egui::Ui, commands: &mut Vec<AppCommand>) {
        let mut open: Option<PathBuf> = None;
        egui::ScrollArea::vertical()
            .id_salt("search_results_scroll")
            .max_height(240.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for res in &self.file_results {
                    let rel = res.path
                        .display()
                        .to_string();
                    ui.horizontal(|ui| {
                        let label = ui
                            .add(
                                egui::Label::new(
                                    egui::RichText::new(format!("{}:{}", res.line_no, res.col))
                                        .monospace(),
                                )
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_text(&rel);
                        if label.clicked() {
                            open = Some(res.path.clone());
                        }
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&res.line_text).color(ui.visuals().text_color()),
                            )
                            .truncate(),
                        );
                    });
                }
            });
        if let Some(p) = open {
            commands.push(AppCommand::OpenFilePath(p));
        }
    }

    fn run_file_search(&mut self, root: Option<&PathBuf>, query: &str, case: bool) {
        self.file_results.clear();
        if query.is_empty() {
            return;
        }
        let Some(root) = root else {
            return;
        };
        if !root.is_dir() {
            return;
        }
        let mut dirs = vec![root.clone()];
        while let Some(dir) = dirs.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.file_name().map(|n| n.to_string_lossy().starts_with('.')).unwrap_or(false) {
                    continue;
                }
                if p.is_dir() {
                    dirs.push(p);
                } else if let Ok(text) = std::fs::read_to_string(&p) {
                    let needle = if case {
                        query.to_string()
                    } else {
                        query.to_lowercase()
                    };
                    for (i, line) in text.lines().enumerate() {
                        let hay = if case {
                            line.to_string()
                        } else {
                            line.to_lowercase()
                        };
                        if let Some(col) = hay.find(&needle) {
                            self.file_results.push(FileResult {
                                path: p.clone(),
                                line_no: (i + 1) as u32,
                                line_text: line.trim().to_string(),
                                col: col + 1,
                            });
                        }
                    }
                }
            }
        }
    }

    fn count_matches(&mut self, content: &str) {
        if self.query.is_empty() {
            self.total_matches = 0;
            self.current_match = 0;
            return;
        }
        self.total_matches = if self.case_sensitive {
            content.matches(&*self.query).count()
        } else {
            content
                .to_lowercase()
                .matches(&self.query.to_lowercase())
                .count()
        };
        if self.current_match > self.total_matches {
            self.current_match = self.total_matches;
        }
        if self.current_match == 0 && self.total_matches > 0 {
            self.current_match = 1;
        }
    }

    fn next_match(&mut self) {
        if self.total_matches > 0 {
            self.current_match = self.current_match % self.total_matches + 1;
        }
    }

    fn prev_match(&mut self) {
        if self.total_matches > 0 {
            self.current_match = if self.current_match <= 1 {
                self.total_matches
            } else {
                self.current_match - 1
            };
        }
    }
}
