use eframe::egui::{self, Ui};
use egui::text::{CCursor, CCursorRange};
use egui_code_editor::CodeEditor;

use crate::tabs::Tab;
use crate::theme::Theme;

pub fn show_editor(ui: &mut Ui, tab: &mut Tab, theme: Theme, editor_bg: &'static str) {
    let mut color_theme = theme.to_color_theme();
    color_theme.bg = editor_bg;
    let mut editor = CodeEditor::default()
        .id_source(&tab.name)
        .with_fontsize(14.0)
        .with_theme(color_theme)
        .with_numlines(true)
        .with_clickable_links(true);

    let mut output = editor.show(ui, &mut tab.content, &tab.syntax);

    if let Some(line) = tab.goto_line.take() {
        let char_idx = char_index_of_line(&tab.content, line);
        let new_range = CCursorRange::one(CCursor::new(char_idx));
        output.state.cursor.set_char_range(Some(new_range));
        output.state.store(ui.ctx(), output.response.id);
    }

    if let Some(range) = output.cursor_range {
        // CCursor.index is a CharIndex newtype (char offset, not byte offset)
        let char_idx = range.primary.index.0;
        let prefix: String = tab.content.chars().take(char_idx).collect();
        if let Some((line, col)) = line_col_of(&prefix) {
            tab.cursor_line = line;
            tab.cursor_col = col;
        }
    }
}

fn char_index_of_line(content: &str, line: usize) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut count = 0usize;
    for (i, l) in content.lines().enumerate() {
        if i + 1 >= line {
            break;
        }
        count += l.chars().count() + 1;
    }
    count
}

fn line_col_of(text: &str) -> Option<(usize, usize)> {
    match text.rfind('\n') {
        Some(last_newline) => {
            let line = text[..last_newline].matches('\n').count() + 1;
            let col = text[last_newline + 1..].chars().count();
            Some((line, col))
        }
        None => Some((0, text.chars().count())),
    }
}
