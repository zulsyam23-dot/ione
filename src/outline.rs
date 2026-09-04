use eframe::egui::{self, Align, Layout, RichText};
use egui_code_editor::Syntax;

use crate::style::{header_label, Palette};

#[derive(Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: &'static str,
    pub line: usize,
    pub depth: usize,
}

pub struct OutlinePanel;

impl OutlinePanel {
    pub fn new() -> Self {
        Self
    }

    pub fn show(
        &self,
        ui: &mut egui::Ui,
        palette: &Palette,
        content: &str,
        syntax: &Syntax,
    ) -> Option<usize> {
        let syms = extract_symbols(content, syntax);
        ui.horizontal(|ui| {
            ui.label(header_label(ui, "Outline"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} items", syms.len()))
                        .size(11.0)
                        .color(palette.text_muted),
                );
            });
        });
        ui.add_space(6.0);

        if syms.is_empty() {
            ui.label(RichText::new("No symbols found").color(palette.text_muted));
            return None;
        }

        let mut goto: Option<usize> = None;
        egui::ScrollArea::vertical()
            .id_salt("outline_scroll")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for sym in &syms {
                    let row = ui
                        .horizontal(|ui| {
                            ui.add_space(sym.depth as f32 * 9.0);
                            let icon = match sym.kind {
                                "fn" | "function" => "ƒ",
                                "struct" | "class" => "▣",
                                "enum" => "▤",
                                "trait" => "◆",
                                "impl" => "▭",
                                "mod" => "▦",
                                "const" | "static" => "◈",
                                "type" => "=",
                                _ => "·",
                            };
                            ui.label(RichText::new(icon).color(palette.text_muted).monospace());
                            let clicked = ui
                                .add(
                                    egui::Label::new(RichText::new(&sym.name).color(palette.text))
                                        .sense(egui::Sense::click()),
                                )
                                .clicked();
                            if clicked {
                                goto = Some(sym.line);
                            }
                        })
                        .response;
                    if row.hovered() {
                        ui.painter().rect_filled(row.rect, 0.0, palette.panel_hover);
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(sym.depth as f32 * 9.0 + 14.0);
                        ui.label(
                            RichText::new(format!(":{}", sym.line))
                                .size(11.0)
                                .color(palette.text_muted),
                        );
                    });
                }
            });
        goto
    }
}

fn extract_symbols(content: &str, syntax: &Syntax) -> Vec<Symbol> {
    match syntax.language() {
        "Rust" => extract_rust(content),
        "Python" => extract_python(content),
        "Lua" | "Shell" | "Sql" => extract_generic(content),
        _ => Vec::new(),
    }
}

fn push(syms: &mut Vec<Symbol>, line: &str, lineno: usize, kind: &'static str, name: &str) {
    syms.push(Symbol {
        name: name.to_string(),
        kind,
        line: lineno,
        depth: indent_of(line),
    });
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn extract_rust(content: &str) -> Vec<Symbol> {
    let mut syms = Vec::new();
    for (i, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.starts_with("//") || line.is_empty() {
            continue;
        }
        if let Some(name) = after_kw(line, "fn") {
            push(&mut syms, raw, i + 1, "fn", name);
        } else if let Some(name) = after_kw(line, "struct") {
            push(&mut syms, raw, i + 1, "struct", name);
        } else if let Some(name) = after_kw(line, "enum") {
            push(&mut syms, raw, i + 1, "enum", name);
        } else if let Some(name) = after_kw(line, "trait") {
            push(&mut syms, raw, i + 1, "trait", name);
        } else if let Some(name) = after_kw(line, "impl") {
            push(&mut syms, raw, i + 1, "impl", name);
        } else if let Some(name) = after_kw(line, "mod") {
            push(&mut syms, raw, i + 1, "mod", name);
        } else if let Some(name) = after_kw(line, "type") {
            push(&mut syms, raw, i + 1, "type", name);
        } else if let Some(name) = after_const(line) {
            push(&mut syms, raw, i + 1, "const", name);
        }
    }
    syms
}

fn extract_python(content: &str) -> Vec<Symbol> {
    let mut syms = Vec::new();
    for (i, raw) in content.lines().enumerate() {
        let line = raw.trim_start();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = after_kw(line, "def") {
            push(&mut syms, raw, i + 1, "function", name);
        } else if let Some(name) = after_kw(line, "class") {
            push(&mut syms, raw, i + 1, "class", name);
        }
    }
    syms
}

fn extract_generic(content: &str) -> Vec<Symbol> {
    let mut syms = Vec::new();
    for (i, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("--") || line.starts_with('#') {
            continue;
        }
        if let Some(name) = after_kw(line, "function") {
            push(&mut syms, raw, i + 1, "function", name);
        } else if let Some(name) = after_kw(line, "local function") {
            push(&mut syms, raw, i + 1, "function", name);
        } else if line.contains('(') && line.contains(')') && !line.contains(' ') {
            let name: String = line.chars().take_while(|&c| c != '(').collect();
            push(&mut syms, raw, i + 1, "function", &name);
        }
    }
    syms
}

fn after_kw<'a>(line: &'a str, kw: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(kw)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let name: &str = rest
        .trim_start()
        .split(|c: char| c.is_whitespace() || c == '(' || c == '<')
        .next()?;
    let name = name.trim_end_matches(':');
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn after_const(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("const")?;
    if !rest.starts_with(char::is_whitespace) && !rest.starts_with(' ') {
        return None;
    }
    let mut name = rest.trim_start();
    if name.starts_with("unsafe") {
        name = name.strip_prefix("unsafe")?.trim_start();
    }
    let name: &str = name.split(char::is_whitespace).next()?;
    let name = name.trim_end_matches(':');
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_symbols() {
        let src = "fn main() {}\nstruct Foo<T> {}\nimpl Foo {}\nenum Color {}\nconst X: i32 = 1;\n// fn comment\n";
        let s = extract_symbols(src, &egui_code_editor::Syntax::rust());
        let names: Vec<&str> = s.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["main", "Foo", "Foo", "Color", "X"]);
        assert_eq!(s[0].line, 1);
        assert_eq!(s[0].kind, "fn");
    }

    #[test]
    fn python_symbols() {
        let src = "def add(a, b):\n    return a + b\n\nclass Dog:\n    def bark(self):\n";
        let s = extract_symbols(src, &egui_code_editor::Syntax::python());
        let names: Vec<&str> = s.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["add", "Dog", "bark"]);
        assert_eq!(s[2].depth, 4);
    }
}
