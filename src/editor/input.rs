//! Pre-frame input preprocessing for the code editor: claims and rewrites the
//! raw event stream *before* `TextEdit` sees it. Stages run in sequence and
//! are mutually exclusive by editor state:
//!
//! - [`handle_multi_claim`] — one keystroke edits every cursor range at once;
//! - [`handle_completion_claim`] — ↑/↓/Enter/Tab/Esc pick or close the popup;
//! - [`text_editing_pass`] — auto-close `()[]{}` and quotes, auto-indent a
//!   plain Enter, and the whole-line ops (Ctrl+/, Ctrl+Shift+D/K, Alt+↑/↓).
//!
//! Everything operates on the REAL buffer: pair closures and the indent are
//! spliced into this frame's events so the one `TextEdit` pass inserts them;
//! the line-op keys are removed so `TextEdit` never acts on them and the ops
//! run after the pass (see `show_editor`).

use eframe::egui::{self, Event};
use egui::widgets::text_edit::TextEditState;

use crate::completion;
use crate::tabs::Tab;

use super::multi;

/// Whole-line keyboard operations dispatched in the editor's event pass.
#[derive(Clone, Copy)]
pub(crate) enum LineOp {
    Comment,
    Duplicate,
    Delete,
    Move(i32),
}

/// Matching closing char for an auto-closed bracket/quote, or `None`.
fn close_pair(c: char) -> Option<(char, char)> {
    match c {
        '(' => Some(('(', ')')),
        '[' => Some(('[', ']')),
        '{' => Some(('{', '}')),
        '"' => Some(('"', '"')),
        '\'' => Some(('\'', '\'')),
        '`' => Some(('`', '`')),
        _ => None,
    }
}

/// Leading ` ` / `\t` run of the line containing char index `caret`.
fn line_indent(content: &str, caret: usize) -> String {
    let line: String = content.chars().take(caret).collect();
    line.rsplit('\n')
        .next()
        .unwrap_or("")
        .chars()
        .take_while(|&c| c == ' ' || c == '\t')
        .collect()
}

/// Multi-select batch edit: while a multi-select is active, a single keystroke
/// edits every range at once. Only that one event is consumed (egui's
/// single-cursor `TextEdit` must not apply it twice); any click or arrow key
/// cancels the mode and passes through normally. After the edit the carets
/// move to the end of each change so further typing appends.
pub(crate) fn handle_multi_claim(ui: &mut egui::Ui, editor_id: &str, tab: &mut Tab) {
    let Some(multi) = tab.multi.clone() else {
        return;
    };
    let focus_ok = ui
        .ctx()
        .memory(|m| m.focused().is_some_and(|f| f == egui::Id::new(editor_id)));
    let mut edit: Option<multi::Edit> = None;
    if focus_ok {
        edit = ui.ctx().input_mut(|i| {
            for (n, ev) in i.events.iter().enumerate() {
                if matches!(
                    ev,
                    Event::PointerButton { pressed: true, .. }
                        | Event::Key {
                            key:
                                egui::Key::ArrowLeft
                                | egui::Key::ArrowRight
                                | egui::Key::ArrowUp
                                | egui::Key::ArrowDown
                                | egui::Key::Home
                                | egui::Key::End,
                            pressed: true,
                            ..
                        }
                ) {
                    tab.multi = None;
                    return None;
                }
                if multi::is_batchable(ev) {
                    let e = match ev {
                        Event::Text(t) => multi::Edit::Insert(t.clone()),
                        Event::Key { key: egui::Key::Backspace, .. } => multi::Edit::Backspace,
                        Event::Key { key: egui::Key::Delete, .. } => multi::Edit::Delete,
                        Event::Key { key: egui::Key::Enter, .. } => multi::Edit::Enter,
                        Event::Key { key: egui::Key::Tab, .. } => multi::Edit::InsertTab,
                        _ => unreachable!(),
                    };
                    i.events.remove(n);
                    return Some(e);
                }
            }
            None
        });
    }
    if let Some(edit) = edit {
        let ranges = multi.ranges.clone();
        if let Some(carets) = multi::apply_edit(&mut tab.content, &ranges, &edit) {
            tab.dirty = true;
            if let Some(m) = &mut tab.multi {
                m.ranges = carets;
            }
        }
    }
}

/// Autocomplete claim: while the popup is open, pick/accept/close keys are
/// consumed here so `TextEdit` never sees them — ↑/↓ must not move the caret
/// while a suggestion is being selected, Enter/Tab must not insert a
/// newline/tab, Esc must not leak to the app menu. Click and caret moves close
/// it too. Mutually exclusive with the multi-select claim on the same keys.
pub(crate) fn handle_completion_claim(ui: &mut egui::Ui, editor_id: &str, tab: &mut Tab) {
    if tab.multi.is_some() {
        return;
    }
    let focus_ok = ui
        .ctx()
        .memory(|m| m.focused().is_some_and(|f| f == egui::Id::new(editor_id)));
    let action = if focus_ok && tab.completion.as_ref().is_some_and(|c| !c.items.is_empty()) {
        ui.ctx().input_mut(|i| {
            for (n, ev) in i.events.iter().enumerate() {
                let take = match ev {
                    Event::Key { key: egui::Key::ArrowUp, pressed: true, .. } => {
                        Some(completion::Action::Prev)
                    }
                    Event::Key { key: egui::Key::ArrowDown, pressed: true, .. } => {
                        Some(completion::Action::Next)
                    }
                    Event::Key {
                        key: egui::Key::Enter | egui::Key::Tab,
                        pressed: true,
                        repeat: false,
                        ..
                    } => Some(completion::Action::Accept),
                    Event::Key { key: egui::Key::Escape, pressed: true, repeat: false, .. } => {
                        Some(completion::Action::Close)
                    }
                    Event::Key {
                        key:
                            egui::Key::ArrowLeft
                            | egui::Key::ArrowRight
                            | egui::Key::Home
                            | egui::Key::End,
                        pressed: true,
                        ..
                    }
                    | Event::PointerButton { pressed: true, .. } => {
                        Some(completion::Action::Close)
                    }
                    _ => None,
                };
                if let Some(a) = take {
                    i.events.remove(n);
                    return Some(a);
                }
            }
            None
        })
    } else {
        None
    };
    let next = action
        .as_ref()
        .is_some_and(|a| matches!(a, completion::Action::Next));
    match action {
        Some(completion::Action::Prev) | Some(completion::Action::Next) => {
            if let Some(st) = &mut tab.completion {
                let n = st.items.len();
                if n > 1 {
                    st.selected = if next {
                        (st.selected + 1) % n
                    } else {
                        (st.selected + n - 1) % n
                    };
                }
            }
        }
        Some(completion::Action::Accept) => {
            if let Some(st) = tab.completion.take() {
                if let Some(item) = st.items.get(st.selected) {
                    let tail = item.tail.clone();
                    tab.dirty = true;
                    // Spliced into this frame's events: `TextEdit` pastes the
                    // tail at the caret, completing the word on the spot.
                    ui.ctx().input_mut(|i| i.events.push(Event::Paste(tail)));
                }
            }
        }
        Some(completion::Action::Close) => tab.completion = None,
        None => {}
    }
}

/// Text-editing pass — single cursor only (the multi-select claim above
/// already edits its own events). Auto-close `()[]{} """'`` ` pairs,
/// auto-indent a plain Enter, and the whole-line shortcuts (Ctrl+/,
/// Ctrl+Shift+D/K, Alt+↑/↓). Everything works on the REAL buffer: pair
/// closures and the indent are spliced into this frame's events so the one
/// TextEdit pass inserts them; the line-op keys are removed so TextEdit
/// never acts on them, and the ops run after the pass below.
///
/// Returns `(pending_wrap_close, pending_lineop)` for the caller's post-frame
/// handling.
pub(crate) fn text_editing_pass(
    ui: &mut egui::Ui,
    editor_id: &str,
    tab: &mut Tab,
) -> (Option<char>, Option<LineOp>) {
    if tab.multi.is_some() {
        return (None, None);
    }
    let focus_ok = ui
        .ctx()
        .memory(|m| m.focused().is_some_and(|f| f == egui::Id::new(editor_id)));
    if !focus_ok {
        return (None, None);
    }

    // Cheap gate: nothing below matters unless a single-char keystroke or a
    // relevant key actually arrived. Skips the O(content) cursor context on
    // every idle frame while the editor holds focus (was a flat regression).
    let relevant = ui.ctx().input(|i| {
        i.events.iter().any(|ev| {
            matches!(ev, egui::Event::Text(t) if t.chars().count() == 1)
                || matches!(ev, egui::Event::Key { pressed: true, repeat: false, .. })
        })
    });
    if !relevant {
        return (None, None);
    }
    let is_word = completion::is_word_char;

    enum EventEdit {
        Replace(Event),
        Insert(Event),
    }
    let mut wrap_close = None::<char>;
    let mut lineop = None::<LineOp>;
    ui.ctx().input_mut(|i| {
        let mut edits: Vec<(usize, EventEdit)> = Vec::new();
        let mut remove = None::<usize>;
        // (has_sel, caret, prev, next, mask) — built once, only once a 1-char
        // Text event actually arrives.
        let mut ctx = None::<(bool, usize, Option<char>, Option<char>, &[bool])>;
        for (n, ev) in i.events.iter().enumerate() {
            match ev {
                Event::Text(t) if t.chars().count() == 1 => {
                    let (has_sel, caret, prev, next, mask) = *ctx.get_or_insert_with(|| {
                        let has_sel = TextEditState::load(ui.ctx(), egui::Id::new(editor_id))
                            .and_then(|st| st.cursor.char_range())
                            .is_some_and(|r| !r.is_empty());
                        let caret = tab.cursor_char;
                        let chars: Vec<char> = tab.content.chars().collect();
                        let (prev, next) = (
                            caret.checked_sub(1).and_then(|i| chars.get(i).copied()),
                            chars.get(caret).copied(),
                        );
                        (has_sel, caret, prev, next, &tab.cache.mask)
                    });
                    let inside = |ci: usize| mask.get(ci).copied().unwrap_or(false);
                    let c = t.chars().next().unwrap();
                    if has_sel {
                        continue;
                    }
                    if let Some((open, close)) = close_pair(c) {
                        let ok = if matches!(c, '\'' | '"' | '`') {
                            prev.is_none_or(|p| !is_word(p))
                                && next.is_none_or(|n| !is_word(n))
                                && !inside(caret)
                                && !inside(caret.saturating_sub(1))
                        } else {
                            next != Some(close)
                                && !inside(caret)
                                && !inside(caret.saturating_sub(1))
                        };
                        if ok {
                            wrap_close = Some(close);
                            edits.push((
                                n,
                                EventEdit::Replace(Event::Text(format!("{open}{close}"))),
                            ));
                        }
                    }
                }
                Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if !modifiers.ctrl && !modifiers.shift && !modifiers.alt => {
                    // Auto-indent: repeat the current line's leading
                    // whitespace after a plain Enter, when there is any.
                    let ws = line_indent(&tab.content, tab.cursor_char);
                    if !ws.is_empty() {
                        edits.push((n, EventEdit::Insert(Event::Text(ws.clone()))));
                    }
                }
                Event::Key {
                    key: egui::Key::Slash,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if modifiers.ctrl && !modifiers.shift && !modifiers.alt => {
                    remove = remove.or(Some(n));
                    lineop = Some(LineOp::Comment);
                }
                Event::Key {
                    key: egui::Key::D,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if modifiers.ctrl && modifiers.shift => {
                    remove = remove.or(Some(n));
                    lineop = Some(LineOp::Duplicate);
                }
                Event::Key {
                    key: egui::Key::K,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if modifiers.ctrl && modifiers.shift => {
                    remove = remove.or(Some(n));
                    lineop = Some(LineOp::Delete);
                }
                Event::Key {
                    key: egui::Key::ArrowUp,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.alt && !modifiers.ctrl && !modifiers.shift => {
                    remove = remove.or(Some(n));
                    lineop = Some(LineOp::Move(-1));
                }
                Event::Key {
                    key: egui::Key::ArrowDown,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.alt && !modifiers.ctrl && !modifiers.shift => {
                    remove = remove.or(Some(n));
                    lineop = Some(LineOp::Move(1));
                }
                _ => {}
            }
        }
        // Apply back-to-front so earlier indices stay valid.
        edits.sort_by_key(|&(idx, _)| std::cmp::Reverse(idx));
        for (idx, edit) in edits {
            match edit {
                EventEdit::Replace(e) => i.events[idx] = e,
                EventEdit::Insert(e) => i.events.insert(idx + 1, e),
            }
        }
        if let Some(idx) = remove {
            if idx < i.events.len() {
                i.events.remove(idx);
            }
        }
    });
    (wrap_close, lineop)
}