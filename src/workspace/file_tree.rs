use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;

use crate::app::AppCommand;
use crate::workspace::gitignore::Ignores;
use crate::core::icons::{Icon, Icons};
use crate::core::style::Palette;

pub struct FileTree {
    pub root: Option<PathBuf>,
    pub entries: Vec<TreeEntry>,
    pub expanded: Vec<PathBuf>,
}

#[derive(Clone)]
pub struct TreeEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
}

impl FileTree {
    pub fn new() -> Self {
        Self {
            root: None,
            entries: Vec::new(),
            expanded: Vec::new(),
        }
    }

    pub fn set_root(&mut self, root: PathBuf) {
        self.entries.clear();
        self.expanded.clear();
        if root.is_dir() {
            self.expanded.push(root.clone());
            self.root = Some(root.clone());
            let mut ignores = Ignores::new();
            self.scan_dir(&root, 0, &mut ignores);
        }
    }

    fn scan_dir(&mut self, dir: &Path, depth: usize, ignores: &mut Ignores) {
        // Recursive (inline parent→children order in `entries`); depth-capped
        // so a junction loop (Windows) can't recurse forever.
        const MAX_SCAN_DEPTH: usize = 256;
        if depth > MAX_SCAN_DEPTH {
            return;
        }
        ignores.enter(dir);
        let mut entries: Vec<_> = match std::fs::read_dir(dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                // Dotfiles stay hidden, except `.gitignore`: it is a real
                // project file, and being able to open and edit it is the
                // point of honouring it in the first place.
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    !name.starts_with('.') || name.eq_ignore_ascii_case(".gitignore")
                })
                .collect(),
            Err(_) => return,
        };

        entries.sort_by(|a, b| {
            let a_dir = a.path().is_dir();
            let b_dir = b.path().is_dir();
            b_dir
                .cmp(&a_dir)
                .then_with(|| a.file_name().cmp(&b.file_name()))
        });

        for entry in entries {
            let path = entry.path();
            let is_dir = path.is_dir();
            // Ignored paths are dropped whole, subtrees included.
            if ignores.is_ignored(&path, is_dir) {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            self.entries.push(TreeEntry {
                path: path.clone(),
                name,
                is_dir,
                depth,
            });

            if is_dir && self.expanded.contains(&path) {
                self.scan_dir(&path, depth + 1, ignores);
            }
        }
        ignores.leave(dir);
    }

    pub fn refresh(&mut self) {
        if let Some(root) = self.root.clone() {
            self.entries.clear();
            let mut ignores = Ignores::new();
            self.scan_dir(&root, 0, &mut ignores);
        }
    }

    /// Number of folders directly under `parent` (or the root when None) —
    /// used to prefill unique `untitled-folder-N` names.
    pub fn count_dirs(&self, parent: Option<&Path>) -> usize {
        self.entries
            .iter()
            .filter(|e| e.is_dir)
            .filter(|e| match parent {
                Some(p) => e.path.parent() == Some(p),
                None => e.depth == 0,
            })
            .count()
    }

    /// Number of files directly under `parent` — used to prefill unique
    /// `untitled-N.rs` file names without colliding.
    pub fn count_files(&self, parent: Option<&Path>) -> usize {
        self.entries
            .iter()
            .filter(|e| !e.is_dir)
            .filter(|e| match parent {
                Some(p) => e.path.parent() == Some(p),
                None => e.depth == 0,
            })
            .count()
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        icons: &mut Icons,
        palette: &Palette,
        commands: &mut Vec<AppCommand>,
        tints: Option<&HashMap<PathBuf, egui::Color32>>,
    ) {
        if self.root.is_none() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label("No folder open");
                ui.add_space(8.0);
                if ui.button("Open Folder").clicked() {
                    commands.push(AppCommand::OpenFolder);
                }
            });
            return;
        }

        let entries: Vec<TreeEntry> = self.entries.clone();
        let mut clicked_path: Option<PathBuf> = None;
        let mut toggle_path: Option<PathBuf> = None;

        egui::ScrollArea::vertical()
            .id_salt("file_tree_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for entry in &entries {
                    // Cap visual indent so deep folders can't push rows past the panel
                    // edge (which made `available_width` go negative and stopped the
                    // rows rendering).
                    const MAX_INDENT: f32 = 256.0;
                    let indent = (entry.depth as f32 * 16.0).min(MAX_INDENT);

                    // Full-row hover highlight like Zed's file explorer.
                    let row_height = ui.spacing().interact_size.y + 2.0;
                    let row_rect = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width().max(0.0), row_height),
                    );
                    let hovered = ui.rect_contains_pointer(row_rect);
                    let fill = if hovered {
                        palette.panel_hover
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let stroke = egui::Stroke::NONE;
                    let hover_rect = row_rect.shrink2(egui::vec2(4.0, 1.0));
                    ui.painter()
                        .rect(hover_rect, 5.0, fill, stroke, egui::StrokeKind::Inside);

                    ui.horizontal(|ui| {
                        if entry.is_dir {
                            // Left-click "+": fixed left column on every folder row, so
                            // nesting never shifts it sideways (and never the scrollbar).
                            let plus = ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("+")
                                            .size(14.0)
                                            .color(palette.text_muted),
                                    )
                                    .frame(false),
                                )
                                .on_hover_text("Add file or folder");
                            egui::Popup::menu(&plus).show(|ui| {
                                if ui.button("New File").clicked() {
                                    commands.push(AppCommand::NewFileIn(entry.path.clone()));
                                    ui.close();
                                }
                                if ui.button("New Folder").clicked() {
                                    commands.push(AppCommand::NewFolder(Some(entry.path.clone())));
                                    ui.close();
                                }
                            });
                        } else {
                            // Keep the icon columns aligned with folder rows above.
                            ui.add_space(16.0);
                        }
                        ui.add_space(indent);
                        if entry.is_dir {
                            let chevron = if self.expanded.contains(&entry.path) {
                                Icon::ChevronDown
                            } else {
                                Icon::ChevronRight
                            };
                            icons.image_button(ui, chevron, 16.0, "");
                        } else {
                            ui.add_space(16.0);
                        }

                        let file_icon = if entry.is_dir {
                            Icon::Folder
                        } else {
                            file_icon(&entry.name)
                        };
                        icons.image_button(ui, file_icon, 16.0, "");

                        let color = if let Some(t) = tints.and_then(|m| m.get(&entry.path)) {
                            *t
                        } else if entry.is_dir {
                            palette.text
                        } else {
                            palette.text_muted
                        };
                        ui.label(egui::RichText::new(&entry.name).color(color));
                    });

                    let row_id = ui.make_persistent_id(&entry.path);
                    let row_resp = ui.interact(row_rect, row_id, egui::Sense::click());
                    if row_resp.clicked() {
                        if entry.is_dir {
                            toggle_path = Some(entry.path.clone());
                        } else {
                            clicked_path = Some(entry.path.clone());
                        }
                    }
                    row_resp.context_menu(|ui| {
                        if entry.is_dir {
                            if ui.button("New File").clicked() {
                                commands.push(AppCommand::NewFileIn(entry.path.clone()));
                                ui.close();
                            }
                            if ui.button("New Folder").clicked() {
                                commands.push(AppCommand::NewFolder(Some(entry.path.clone())));
                                ui.close();
                            }
                        }
                        if ui.button("Open").clicked() {
                            if entry.is_dir {
                                toggle_path = Some(entry.path.clone());
                            } else {
                                commands.push(AppCommand::OpenFilePath(entry.path.clone()));
                            }
                            ui.close();
                        }
                        if ui.button("Rename").clicked() {
                            commands.push(AppCommand::RenamePath(entry.path.clone()));
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            commands.push(AppCommand::DeletePath(entry.path.clone()));
                            ui.close();
                        }
                        if ui.button("Copy Path").clicked() {
                            commands.push(AppCommand::CopyPath(entry.path.clone()));
                            ui.close();
                        }
                    });
                    ui.add_space(1.0);
                }
            });

        if let Some(path) = toggle_path {
            if let Some(pos) = self.expanded.iter().position(|p| *p == path) {
                self.expanded.remove(pos);
            } else {
                self.expanded.push(path);
            }
            self.refresh();
        }

        if let Some(path) = clicked_path {
            commands.push(AppCommand::OpenFilePath(path));
        }
    }
}

fn file_icon(name: &str) -> Icon {
    // Licenses are recognised by name rather than extension: `LICENSE`,
    // `LICENSE.md`, `COPYING` and `NOTICE` all mean the same thing, and the
    // point of the icon is to spot them at a glance.
    let stem = name.split('.').next().unwrap_or(name).to_ascii_lowercase();
    if matches!(stem.as_str(), "license" | "licence" | "copying" | "notice") {
        return Icon::LangLicense;
    }
    // `.gitignore` has no extension to key off: `rsplit` would hand back the
    // whole dot name, so match the file name itself.
    let lower = name.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        ".gitignore" | ".npmignore" | ".dockerignore"
    ) {
        return Icon::LangGitignore;
    }
    match name.rsplit('.').next() {
        Some("rs") => Icon::LangRust,
        Some("py") => Icon::LangPython,
        Some("js" | "mjs" | "cjs") => Icon::LangJs,
        Some("ts" | "mts" | "cts") => Icon::Code,
        Some("html" | "htm") => Icon::LangHtml,
        Some("css") => Icon::LangCss,
        Some("json") => Icon::LangJson,
        Some("toml") => Icon::LangToml,
        Some("yaml" | "yml") => Icon::LangYaml,
        Some("md" | "markdown") => Icon::LangMarkdown,
        Some("lua") => Icon::LangLua,
        Some("sh" | "bash" | "zsh") => Icon::LangShell,
        Some("sql") => Icon::LangSql,
        Some("c" | "h") => Icon::LangC,
        Some("lock") => Icon::LangConfig,
        _ => Icon::LangPlain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn license_files_get_their_own_icon() {
        for name in [
            "LICENSE",
            "license",
            "LICENCE",
            "LICENSE.md",
            "License.txt",
            "COPYING",
            "NOTICE",
        ] {
            assert!(matches!(file_icon(name), Icon::LangLicense), "{name}");
        }
    }

    #[test]
    fn other_documents_keep_their_own_icons() {
        assert!(matches!(file_icon("README.md"), Icon::LangMarkdown));
        assert!(matches!(file_icon("notes.txt"), Icon::LangPlain));
        assert!(matches!(file_icon("main.rs"), Icon::LangRust));
    }

    #[test]
    fn ignore_files_get_their_own_icon() {
        for name in [".gitignore", ".GITIGNORE", ".npmignore", ".dockerignore"] {
            assert!(matches!(file_icon(name), Icon::LangGitignore), "{name}");
        }
    }

    #[test]
    fn the_scan_hides_gitignored_paths_but_keeps_the_ignore_file() {
        let dir = std::env::temp_dir().join(format!("ione_ignore_test_{}", std::process::id()));
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join(".gitignore"), "target/\n*.log\n").unwrap();
        std::fs::write(src.join("main.rs"), "").unwrap();
        std::fs::write(src.join("debug.log"), "").unwrap();

        let mut tree = FileTree::new();
        tree.set_root(dir.clone());
        tree.expanded.push(src.clone());
        tree.refresh();

        let names: Vec<String> = tree.entries.iter().map(|e| e.name.clone()).collect();
        assert!(names.contains(&"main.rs".to_string()), "{names:?}");
        assert!(names.contains(&"src".to_string()), "{names:?}");
        // The `.gitignore` is a real project file: it has to be openable.
        assert!(names.contains(&".gitignore".to_string()), "{names:?}");
        assert!(!names.contains(&"target".to_string()), "{names:?}");
        assert!(!names.contains(&"debug.log".to_string()), "{names:?}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn expanded_children_stay_inline_under_their_folder() {
        let dir = std::env::temp_dir().join(format!("ione_tree_test_{}", std::process::id()));
        let sub = dir.join("a");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("root.rs"), "").unwrap();
        std::fs::write(sub.join("inner.rs"), "").unwrap();

        let mut tree = FileTree::new();
        tree.set_root(dir.clone());
        tree.expanded.push(sub.clone());
        tree.refresh();

        let names: Vec<String> = tree.entries.iter().map(|e| e.name.clone()).collect();
        let a = names
            .iter()
            .position(|n| n == "a")
            .expect("folder a listed");
        let inner = names
            .iter()
            .position(|n| n == "inner.rs")
            .expect("child below a");
        let root = names
            .iter()
            .position(|n| n == "root.rs")
            .expect("root file listed");
        // The folder's children must appear immediately under the folder, before\
        // the parent's other (unexpanded) files.
        assert!(a < inner && inner < root, "order was {names:?}");

        std::fs::remove_dir_all(&dir).ok();
    }
}
