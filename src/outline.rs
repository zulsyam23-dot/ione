use eframe::egui::{self, Align, Layout, RichText};
use egui_code_editor::Syntax;

use crate::style::{header_label, Palette};

#[derive(Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: &'static str,
    pub line: usize,
    pub depth: usize,
    /// Paren group right after the name for callable symbols (`(a, b)`), empty
    /// otherwise — feeds the completion popup's signature detail.
    pub params: String,
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
        symbols: &[Symbol],
    ) -> Option<usize> {
        ui.horizontal(|ui| {
            ui.label(header_label(ui, "Outline"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} items", symbols.len()))
                        .size(11.0)
                        .color(palette.text_muted),
                );
            });
        });
        ui.add_space(6.0);

        if symbols.is_empty() {
            ui.label(RichText::new("No symbols found").color(palette.text_muted));
            return None;
        }

        let mut goto: Option<usize> = None;
        egui::ScrollArea::vertical()
            .id_salt("outline_scroll")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for sym in symbols {
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

pub(crate) fn extract_symbols(content: &str, syntax: &Syntax) -> Vec<Symbol> {
    match syntax.language() {
        "Rust" => extract_rust(content),
        "Python" => extract_python(content),
        "Lua" | "Shell" | "Sql" => extract_generic(content),
        _ => Vec::new(),
    }
}

fn push(syms: &mut Vec<Symbol>, line: &str, lineno: usize, kind: &'static str, name: &str) {
    push_with_params(syms, line, lineno, kind, name, String::new());
}

fn push_with_params(
    syms: &mut Vec<Symbol>,
    line: &str,
    lineno: usize,
    kind: &'static str,
    name: &str,
    params: String,
) {
    syms.push(Symbol {
        name: name.to_string(),
        kind,
        line: lineno,
        depth: indent_of(line),
        params,
    });
}

/// The first balanced paren group in `line` (`(a: i32, b: i32)`), or empty —
/// used for function-like symbols.
fn params_of(line: &str) -> String {
    let Some(open) = line.find('(') else {
        return String::new();
    };
    let text = &line[open..];
    let mut depth = 0usize;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let group = &text[..i + c.len_utf8()];
                    // `fn main() {}` has no params — don't report an empty `()`.
                    return if group.len() == 2 {
                        String::new()
                    } else {
                        group.to_string()
                    };
                }
            }
            _ => {}
        }
    }
    text.to_string()
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
            push_with_params(&mut syms, raw, i + 1, "fn", name, params_of(line));
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
            push_with_params(&mut syms, raw, i + 1, "function", name, params_of(line));
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
            push_with_params(&mut syms, raw, i + 1, "function", name, params_of(line));
        } else if let Some(name) = after_kw(line, "local function") {
            push_with_params(&mut syms, raw, i + 1, "function", name, params_of(line));
        } else if line.contains('(') && line.contains(')') && !line.contains(' ') {
            let name: String = line.chars().take_while(|&c| c != '(').collect();
            push_with_params(&mut syms, raw, i + 1, "function", &name, params_of(line));
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
        let src = "fn main() {}\nfn add(a: i32, b: i32) -> i32 { a + b }\nstruct Foo<T> {}\nimpl Foo {}\nenum Color {}\nconst X: i32 = 1;\n// fn comment\n";
        let s = extract_symbols(src, &egui_code_editor::Syntax::rust());
        let names: Vec<&str> = s.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["main", "add", "Foo", "Foo", "Color", "X"]);
        assert_eq!(s[0].line, 1);
        assert_eq!(s[0].kind, "fn");
        assert_eq!(s[0].params, "");
        assert_eq!(s[1].params, "(a: i32, b: i32)");
    }

    #[test]
    fn python_symbols() {
        let src = "def add(a, b):\n    return a + b\n\nclass Dog:\n    def bark(self):\n";
        let s = extract_symbols(src, &egui_code_editor::Syntax::python());
        let names: Vec<&str> = s.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["add", "Dog", "bark"]);
        assert_eq!(s[2].depth, 4);
        assert_eq!(s[0].params, "(a, b)");
        assert_eq!(s[2].params, "(self)");
    }
}
