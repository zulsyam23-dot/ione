//! Editor widget: the layouter bakes per-bracket colors while guides are
//! drawn through the scroll content painter (clipped to the viewport and
//! translated with the scroll, so they never bleed into the surrounding ui
//! and always line up with the glyphs). Same layout/ids as the
//! `egui_code_editor` widget it replaces, so TextEdit state is preserved.
//!
//! Submodules:
//! - `styling` — syntax-coloring pipeline: mask → bracket scan → `LayoutJob`.
//! - `gutter` — line-number gutter widget.
//! - `cursor` — pure text-position math (goto line, line/col tracking).
//! - `links` — clickable hyperlink resolution drawn on top of the text.

use std::cell::RefCell;

use eframe::egui::{self, Event, Ui};
use egui::text::CCursor;
use egui::widgets::text_edit::TextEditOutput;
use egui_code_editor::highlighting::Links;

use crate::completion::{self, CompletionState};
use crate::guides::{draw_editor_overlays, BracketScan, EditorOverlay};
use crate::icons::Icons;
use crate::style::Palette;
use crate::tabs::Tab;
use crate::theme::Theme;

use egui_code_editor::{ColorTheme, TokenType};

use self::folds::FoldView;

pub(crate) mod cursor;
pub(crate) mod folds;
pub(crate) mod gutter;

pub(crate) mod lexer;
pub(crate) mod links;
pub(crate) mod multi;
pub(crate) mod styling;

pub(crate) const FONT_SIZE: f32 = 14.0;
pub(crate) const TEXT_ROWS: usize = 10;
pub(crate) const SPACE_HOLDER: &str = "â£";

/// Draws a code tab: one lexer pass per role (mask+links, then mask-aware
/// bracket scan, then the colored job), scrolled text edit, then guides and
/// clickable links over it, plus jump-to-line / cursor tracking. Folds are a
/// presentation layer: the displayed text hides folded interiors, while every
/// edit lands in the real buffer (see `folds`).
pub fn show_editor(
    ui: &mut Ui,
    tab: &mut Tab,
    theme: Theme,
    editor_bg: &'static str,
    overlay: &EditorOverlay,
    palette: &Palette,
    icons: &mut Icons,
) {
    let mut color_theme = theme.to_color_theme();
    color_theme.bg = editor_bg;
    let editor_id = format!("{}{}", tab.uid, tab.name);
    let syntax = tab.syntax.clone();

    // Everything derived from the content (diagnostics, symbols, mask+links,
    // bracket scan, fold targets) lives in `tab.cache` and is recomputed once
    // per content/style change, not every frame (see `Tab::refresh_cache`).
    tab.refresh_cache(theme, editor_bg, overlay, palette);

    // Real-content structure: prunes stale folds, lists foldable rows, and
    // lets guides suppress pairs whose match is hidden behind a fold.
    let valid: std::collections::HashSet<usize> = tab
        .cache
        .scan
        .brace_pairs
        .iter()
        .filter(|b| b.close != usize::MAX)
        .map(|b| b.open)
        .collect();

    tab.folds.retain(|f| valid.contains(&f.open));

    // Multi-select batch edit: while a multi-select is active, a single
    // keystroke edits every range at once. Only that one event is consumed
    // (egui's single-cursor TextEdit must not apply it twice); any click or
    // arrow key cancels the mode and passes through normally. After the edit
    // the carets move to the end of each change so further typing appends.
    if let Some(ref multi) = tab.multi.clone() {
        let focus_ok = ui
            .ctx()
            .memory(|m| m.focused().is_some_and(|f| f == egui::Id::new(&editor_id)));
        let mut edit: Option<multi::Edit> = None;
        if focus_ok {
            edit = ui.ctx().input_mut(|i| {
                for (n, ev) in i.events.iter().enumerate() {
                    if matches!(
                        ev,
                        Event::PointerButton { pressed: true, .. }
                            | Event::Key { key: egui::Key::ArrowLeft | egui::Key::ArrowRight | egui::Key::ArrowUp | egui::Key::ArrowDown | egui::Key::Home | egui::Key::End, pressed: true, .. }
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

    // Autocomplete: while the popup is open, pick/accept/close keys are claimed
    // here so TextEdit never sees them — ↑/↓ must not move the caret while a
    // suggestion is being selected, Enter/Tab must not insert a newline/tab,
    // Esc must not leak to the app menu. Click and caret moves close it too.
    // Mutually exclusive with the multi-select claim on the same keys above.
    if tab.multi.is_none() {
        let focus_ok = ui
            .ctx()
            .memory(|m| m.focused().is_some_and(|f| f == egui::Id::new(&editor_id)));
        let action = if focus_ok && tab.completion.as_ref().is_some_and(|c| !c.items.is_empty()) {
            ui.ctx().input_mut(|i| {
                for (n, ev) in i.events.iter().enumerate() {
                    let take = match ev {
                        Event::Key { key: egui::Key::ArrowUp, pressed: true, .. } => Some(completion::Action::Prev),
                        Event::Key { key: egui::Key::ArrowDown, pressed: true, .. } => Some(completion::Action::Next),
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
                        | Event::PointerButton { pressed: true, .. } => Some(completion::Action::Close),
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
                        // Spliced into this frame's events: TextEdit pastes the
                        // tail at the caret, completing the word on the spot.
                        ui.ctx().input_mut(|i| i.events.push(Event::Paste(tail)));
                    }
                }
            }
            Some(completion::Action::Close) => tab.completion = None,
            None => {}
        }
    }

    // Only well-formed `{ ... }` blocks are fold targets — inline expression
    // braces and multi-line literals (`Foo {\n …\n};`) must not fold. The
    // foldable-opens list comes from `tab.cache` (fold targets don't move
    // while the content is unchanged).

    // Jump-to-line: a hidden target auto-unfolds its fold first.
    let pending_goto = tab.goto_line.take().map(|line| {
        folds::unfold_covering(&mut tab.folds, &tab.content, line);
        line
    });

    // Find-jump: also auto-unfold so the match is never behind a fold, then map
    // real-buffer char indices to display space after the frame is laid out.
    let pending_find = tab.pending_find.take().map(|(s, e)| {
        let line = tab.content.chars().take(s).filter(|&c| c == '\n').count();
        folds::unfold_covering(&mut tab.folds, &tab.content, line);
        (s, e)
    });

    // Display layout for this frame (used by the gutter and the guides). The
    // fold view changes only when the content or the fold set does, so idle
    // frames clone the cached one instead of re-laying the whole file, and the
    // `FoldBuffer` reuses it instead of building the file a second time.
    let content_hash0 = crate::diagnostics::hash_content(&tab.content);
    let fold_fp = folds::fold_fp(&tab.folds);
    let view0 = if tab
        .cache
        .fold_view_cache
        .as_ref()
        .is_some_and(|(h, f, _)| *h == content_hash0 && *f == fold_fp)
    {
        tab.cache.fold_view_cache.as_ref().expect("just checked").2.clone()
    } else {
        let v = folds::build_fold_view(&tab.content, &tab.folds);
        tab.cache.fold_view_cache = Some((content_hash0, fold_fp, v.clone()));
        v
    };
    let fold_rows = folds::fold_rows(&view0, &tab.folds, &tab.cache.fold_opens, &tab.content);
    let pending_goto = pending_goto.and_then(|line| view0.char_of_real_line(line));

    // Gutter markers: display rows with a diagnostic, error winning the color.
    let mut diag_rows: Vec<(usize, crate::diagnostics::Severity)> = Vec::new();
    if !tab.cache.diagnostics.is_empty() {
        let mut rows: Vec<(usize, crate::diagnostics::Severity)> = Vec::new();
        for d in &tab.cache.diagnostics {
            let real_line = crate::diagnostics::line_of_char(&tab.content, d.start);
            if let Some(row) = view0.display_row_of(real_line) {
                rows.push((row, d.severity));
            }
        }
        rows.sort_by_key(|&(r, s)| (r, s == crate::diagnostics::Severity::Warning));
        rows.dedup_by_key(|&mut (r, _)| r);
        diag_rows = rows;
    }

    let text_edit_output = RefCell::new(None::<TextEditOutput>);
    // (links, styled) from the latest layouter run, which is the exact galley.
    let styled = RefCell::new(None::<(Links, styling::Styled)>);
    let fold_view = RefCell::new(None::<FoldView>);

    let code_editor = |ui: &mut Ui| {
        let frame = egui::Frame::new().fill(color_theme.bg());
        frame.show(ui, |ui| {
            ui.horizontal_top(|h| {
                color_theme.modify_style(h, FONT_SIZE);
                let clicked =
                    gutter::numlines_show(h, icons, &view0, &fold_rows, &editor_id, &color_theme, &diag_rows, palette);
                for row in clicked {
                    let real_line = view0.rows[row].line;
                    folds::toggle_fold(&mut tab.folds, &tab.cache.fold_opens, &tab.content, real_line);
                }
                h.add_space(14.0);
                egui::ScrollArea::horizontal()
                    .id_salt(format!("{editor_id}_inner_scroll"))
                    .show(h, |ui| {
                        // Layouter with a per-frame job memo: a layouter call
                        // whose buffer text hash and style key match the last
                        // call replays the cached colored `LayoutJob` (and works
                        // without re-lexing). egui's own galley cache only lasts
                        // one frame, so the expensive part we skip here is the
                        // lexer + mask + bracket scan + LayoutJob construction.
                        let mut layouter =
                            |ui: &Ui, text_buffer: &dyn egui::TextBuffer, _wrap_width: f32| {
                                let text_hash =
                                    crate::diagnostics::hash_content(text_buffer.as_str());
                                let cache_key = tab.cache.style_key;
                                if tab
                                    .cache
                                    .job_cache
                                    .as_ref()
                                    .is_some_and(|(sk, h, _, _, _)| *sk == cache_key && *h == text_hash)
                                {
                                    let (_, _, job, links_, s) =
                                        tab.cache.job_cache.as_ref().expect("checked above");
                                    // `links`/scan match this frame's galley
                                    // (same text), so reuse them as-is.
                                    let _ = styled.replace(Some((links_.clone(), s.clone())));
                                    return ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
                                }
                                let (job, links_, styled_) = styling::layout_styled(
                                    text_buffer.as_str(),
                                    &syntax,
                                    &color_theme,
                                    overlay,
                                    palette,
                                );
                                let _ = styled.replace(Some((links_.clone(), styled_.clone())));
                                let galley = ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
                                tab.cache.job_cache =
                                    Some((cache_key, text_hash, job, links_, styled_));
                                galley
                            };

                        // `view0` (from this frame's cache) double-cheats as the
                        // buffer's display, so an idle frame builds the text
                        // only once. Edits relocate folds and rebuild it.
                        let mut fold_buffer = folds::FoldBuffer::with_view(
                            &mut tab.content,
                            &mut tab.folds,
                            view0,
                        );
                        let text_edit = egui::TextEdit::multiline(&mut fold_buffer)
                            .id_source(&editor_id)
                            .lock_focus(true)
                            .desired_rows(TEXT_ROWS)
                            .desired_width(f32::INFINITY)
                            .frame(egui::Frame::NONE)
                            .layouter(&mut layouter);

                        // egui 0.36 `ccursor_previous_word` underflows on
                        // double-click when a line holds multi-char graphemes
                        // (chars > graphemes) -> `attempt to subtract with
                        // overflow`. Skip the frame instead of dying.
                        let output = match std::panic::catch_unwind(
                            std::panic::AssertUnwindSafe(|| text_edit.show(ui)),
                        ) {
                            Ok(output) => output,
                            Err(e) => {
                                eprintln!("editor: skipped a frame after a widget panic ({e:?})");
                                return;
                            }
                        };

                        let buffer_view = fold_buffer.view().clone();
                        if let Some((links_, s)) = styled.borrow().as_ref() {
                            links::handle_links(&output, links_);
                            let scan =
                                suppress_folded_pairs(&s.scan, &tab.cache.scan, &buffer_view);
                            draw_editor_overlays(
                                ui,
                                &editor_id,
                                &output.galley,
                                output.galley_pos,
                                output.cursor_range,
                                &scan,
                                overlay,
                                palette,
                                FONT_SIZE,
                            );
                            // Multi-select highlight over the active ranges.
                            if let Some(m) = &tab.multi {
                                multi::draw_selection(
                                    ui,
                                    &output.galley,
                                    output.galley_pos,
                                    &buffer_view,
                                    &m.ranges,
                                    egui::Color32::from_rgba_unmultiplied(115, 145, 255, 60),
                                );
                            }
                            // Error/warning squiggles + hover tooltips.
                            crate::diagnostics::draw_squiggles(
                                ui,
                                &editor_id,
                                &output.galley,
                                output.galley_pos,
                                &buffer_view,
                                &tab.cache.diagnostics,
                                palette,
                            );
                            // Autocomplete popup: refresh the list from the
                            // caret (folded/display coords). It only opens on a
                            // freshly typed identifier char at the END of a
                            // word (typing mid-word or after punctuation never
                            // pops it), then closes as soon as the caret moves
                            // off the word or the prefix matches nothing.
                            if let Some(range) = output.cursor_range {
                                let (prefix, next) = completion::prefix_at_cursor(
                                    &buffer_view.display,
                                    range.primary.index.0,
                                );
                                let just_typed_word = ui.ctx().input(|i| {
                                    i.events.iter().any(|ev| {
                                        matches!(ev, Event::Text(t)
                                            if t.chars().count() == 1
                                                && completion::is_word_char(t.chars().next().unwrap_or('_'))
                                        )
                                    })
                                });
                                match &mut tab.completion {
                                    None => {
                                        if just_typed_word
                                            && completion::next_char_allows(next)
                                            && !prefix.is_empty()
                                        {
                                            let syms = &tab.cache.symbols;
                                            let items = completion::build_items(
                                                &tab.content, &syntax, syms, &prefix,
                                            );
                                            if !items.is_empty() {
                                                let selected = items
                                                    .iter()
                                                    .position(|it| it.text == prefix)
                                                    .unwrap_or(0);
                                                tab.completion = Some(CompletionState {
                                                    items,
                                                    selected,
                                                    prefix,
                                                });
                                            }
                                        }
                                    }
                                    Some(st) => {
                                        if !completion::next_char_allows(next)
                                            || prefix.is_empty()
                                            || tab.multi.is_some()
                                        {
                                            tab.completion = None;
                                        } else if st.prefix != prefix {
                                            let syms = &tab.cache.symbols;
                                            let items = completion::build_items(
                                                &tab.content, &syntax, syms, &prefix,
                                            );
                                            if items.is_empty() {
                                                tab.completion = None;
                                            } else {
                                                st.items = items;
                                                st.selected = 0;
                                                st.prefix = prefix;
                                            }
                                        }
                                    }
                                }
                                if let Some(st) = &tab.completion {
                                    let cursor_rect =
                                        output.galley.pos_from_cursor(range.primary);
                                    let anchor =
                                        output.galley_pos + cursor_rect.min.to_vec2();
                                    draw_completion_popup(ui, anchor, st, palette, &color_theme);
                                }
                            }
                        }
                        let _ = fold_view.borrow_mut().replace(buffer_view);
                        let _ = text_edit_output.borrow_mut().replace(output);
                    });
            });
        });
    };

    egui::ScrollArea::vertical()
        .id_salt(format!("{editor_id}_outer_scroll"))
        .show(ui, code_editor);

    let view = fold_view
        .into_inner()
        .unwrap_or_else(|| folds::build_fold_view(&tab.content, &tab.folds));
    let Some(mut output) = text_edit_output.into_inner() else {
        eprintln!("editor: skipped frame; no TextEdit output available");
        return;
    };

    if let Some(char_idx) = pending_goto {
        let new_range = egui::text::CCursorRange::one(CCursor::new(char_idx));
        output.state.cursor.set_char_range(Some(new_range));
        output.state.store(ui.ctx(), output.response.id);
    } else if let Some((s, e)) = pending_find {
        if let (Some(ds), Some(de)) = (
            view.d2r.iter().position(|&r| r == s),
            view.d2r.iter().position(|&r| r == e),
        ) {
            let new_range = egui::text::CCursorRange::two(CCursor::new(de), CCursor::new(ds));
            output.state.cursor.set_char_range(Some(new_range));
            output.state.store(ui.ctx(), output.response.id);
        }
    }

    if let Some(range) = output.cursor_range {
        // CCursor.index is a CharIndex newtype (char offset, not char offset)
        // in *display* space; map back to the real buffer before counting
        // rows, so `cursor_line/col` stay aligned with the file.
        let display_idx = range.primary.index.0;
        let real_idx = view.d2r[display_idx.min(view.d2r.len() - 1)];
        tab.cursor_char = real_idx;
        let prefix: String = tab.content.chars().take(real_idx).collect();
        if let Some((line, col)) = cursor::line_col_of(&prefix) {
            tab.cursor_line = line;
            tab.cursor_col = col;
        }
    }
}

/// Draw the suggestion popup under the caret (`anchor` = caret glyph origin),
/// flipping above it when it would fall out of the visible viewport. Painted
/// through the same scrolled painter as the guides, so it tracks the text.
/// Pure shapes, no widget: clicks pass through to TextEdit (which closes the
/// popup), navigation is keyboard-only.
fn draw_completion_popup(
    ui: &egui::Ui,
    anchor: egui::Pos2,
    st: &completion::CompletionState,
    palette: &Palette,
    theme: &ColorTheme,
) {
    let n = st.items.len();
    if n == 0 {
        return;
    }
    let font = egui::FontId::monospace(FONT_SIZE);
    let row_h = FONT_SIZE + 6.0;
    let pad = 6.0;
    let txt_color = |k: completion::Kind| match k {
        completion::Kind::Keyword => theme.type_color(TokenType::Keyword),
        completion::Kind::Type => theme.type_color(TokenType::Type),
        completion::Kind::Special => theme.type_color(TokenType::Special),
        completion::Kind::Func => theme.type_color(TokenType::Function),
        completion::Kind::Word => palette.text,
    };
    let mut widths: Vec<f32> = Vec::with_capacity(n);
    let mut max_text = 40.0f32;
    for it in &st.items {
        let tw = ui.fonts_mut(|f| {
            f.layout_no_wrap(it.text.clone(), font.clone(), egui::Color32::WHITE)
                .size()
                .x
        });
        let dw = if it.detail.is_empty() {
            0.0
        } else {
            ui.fonts_mut(|f| {
                f.layout_no_wrap(it.detail.clone(), font.clone(), egui::Color32::WHITE)
                    .size()
                    .x
            }) + 10.0
        };
        max_text = max_text.max(tw + dw);
        widths.push(tw);
    }
    let mut pop = egui::Rect::from_min_size(
        anchor + egui::vec2(0.0, FONT_SIZE + 2.0),
        egui::vec2(max_text + pad * 2.0, n as f32 * row_h + 4.0),
    );
    let clip = ui.clip_rect();
    if pop.bottom() > clip.bottom() {
        pop = pop.translate(egui::vec2(0.0, -(FONT_SIZE + 2.0) - row_h - pop.height()));
    }
    ui.painter().rect(
        pop,
        3.0,
        palette.panel,
        egui::Stroke::new(1.0, palette.border),
        egui::StrokeKind::Inside,
    );
    for (i, (it, _w)) in st.items.iter().zip(widths).enumerate() {
        let row = egui::Rect::from_min_size(
            egui::Pos2::new(pop.left(), pop.top() + 2.0 + i as f32 * row_h),
            egui::vec2(pop.width(), row_h),
        );
        if i == st.selected {
            ui.painter()
                .rect_filled(row.shrink(1.0), 2.0, palette.accent.gamma_multiply(0.25));
        }
        ui.painter().text(
            row.min + egui::vec2(pad, 1.0),
            egui::Align2::LEFT_TOP,
            it.text.clone(),
            font.clone(),
            txt_color(it.kind),
        );
        if !it.detail.is_empty() {
            ui.painter().text(
                egui::Pos2::new(pop.right() - pad, row.top() + 1.0),
                egui::Align2::RIGHT_TOP,
                it.detail.clone(),
                font.clone(),
                palette.text_muted,
            );
        }
    }
}

/// Drop bracket pairs that are unmatched in the *display* scan only because a
/// fold hides their closing bracket (their real match exists). Without this a
/// folded block's open `{` would light up a guide straight down the file.
fn suppress_folded_pairs(scan: &BracketScan, real_scan: &BracketScan, view: &FoldView) -> BracketScan {
    let matched: std::collections::HashSet<usize> = real_scan
        .pairs
        .iter()
        .filter(|p| p.close != usize::MAX)
        .map(|p| p.open)
        .collect();
    let mut filtered = scan.clone();
    filtered.pairs.retain(|p| {
        if p.close != usize::MAX {
            return true;
        }
        let real_open = view.d2r.get(p.open).copied().unwrap_or(usize::MAX);
        !matched.contains(&real_open)
    });
    filtered
}

#[cfg(test)]
mod repro {
    use super::*;
    use eframe::egui;
    use egui::text::LayoutJob;
    use egui::TextBuffer;

    fn key(key: egui::Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn ctrl_key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        }
    }

    fn click(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn setup(ctx: &egui::Context) {
        ctx.set_fonts(egui::FontDefinitions::default());
    }

    #[test]
    fn painted_editor_renders_rainbow_brackets() {
        let ctx = egui::Context::default();
        setup(&ctx);

        let mut tab = Tab::new(
            "main.rs",
            "fn f() {\n    let v = vec![1, (2 + 3), {4}];\n    g(v);\n}\n",
            egui_code_editor::Syntax::rust(),
        );
        let overlay = EditorOverlay {
            bracket_guides: true,
            colorize_brackets: true,
        };
        let palette = Palette::dark();

        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                eframe::egui::Pos2::ZERO,
                eframe::egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                show_editor(ui, &mut tab, Theme::GithubDark, "000000", &overlay, &palette, &mut Icons::new());
            });
        });

        let mut colors = std::collections::BTreeSet::new();
        for cs in &output.shapes {
            if let egui::Shape::Text(ts) = &cs.shape {
                for sec in &ts.galley.job.sections {
                    colors.insert(sec.format.color.to_array());
                }
            }
        }
        output.drop_without_applying_deltas();

        assert!(colors.len() >= 4, "painted colors = {colors:?}");
    }

    #[test]
    fn editor_paints_squiggles_for_diagnostics() {
        // A file with an unmatched bracket/trailing whitespace must paint
        // squiggle line segments; a clean file must not.
        let run = |source: &str| {
            let ctx = egui::Context::default();
            setup(&ctx);
            let mut tab = Tab::new(
                "main.rs",
                source,
                egui_code_editor::Syntax::rust(),
            );
            let overlay = EditorOverlay {
                bracket_guides: true,
                colorize_brackets: true,
            };
            let palette = Palette::dark();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    eframe::egui::Pos2::ZERO,
                    eframe::egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    show_editor(ui, &mut tab, Theme::GithubDark, "000000", &overlay, &palette, &mut Icons::new());
                });
            });
            let segments = output
                .shapes
                .iter()
                .filter(|cs| matches!(cs.shape, egui::Shape::LineSegment { .. }))
                .count();
            output.drop_without_applying_deltas();
            segments
        };

        let bad = run("fn f() {\n    let x = (1;\n}\n");
        let clean = run("fn f() {\n    let x = 1;\n}\n");
        assert!(bad > 0, "expect squiggles for an unclosed paren, got {bad}");
        assert_eq!(clean, 0, "a clean file must not paint squiggles");
    }

    fn frame(ctx: &egui::Context, text: &mut String, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                eframe::egui::Pos2::ZERO,
                eframe::egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let mut layouter = |ui: &Ui, text_buffer: &dyn TextBuffer, _wrap_width: f32| {
                    let job = LayoutJob::single_section(
                        text_buffer.as_str().to_string(),
                        styling::format_font(FONT_SIZE, egui::Color32::WHITE),
                    );
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                let te = egui::TextEdit::multiline(&mut *text)
                    .id_source("repro")
                    .lock_focus(true)
                    .desired_rows(TEXT_ROWS)
                    .desired_width(f32::INFINITY)
                    .frame(egui::Frame::NONE)
                    .layouter(&mut layouter);
                te.show(ui);
            });
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn editing_gestures_do_not_panic() {
        let ctx = egui::Context::default();
        setup(&ctx);

        let mut text = String::from("fn main() {\n    let x = 1;\n}\n");
        let big = "fn f() {\n  [1, 2, 3].iter().map(|v| v * v).collect()\n}\n\n"
            .repeat(30);
        let steps = vec![
            vec![click(egui::pos2(120.0, 100.0), true), click(egui::pos2(120.0, 100.0), false)],
            vec![egui::Event::Paste(big.clone())],
            vec![egui::Event::Text("xyz".to_string())],
            vec![key(egui::Key::Backspace, true), key(egui::Key::Backspace, false)],
            vec![key(egui::Key::Enter, true), key(egui::Key::Enter, false)],
            vec![ctrl_key(egui::Key::A)],
            vec![egui::Event::Paste("  } else if (a) {\n    b();\n  }\n".to_string())],
            vec![key(egui::Key::ArrowUp, true), key(egui::Key::ArrowUp, false)],
            vec![key(egui::Key::ArrowDown, true), key(egui::Key::ArrowDown, false)],
            vec![key(egui::Key::Delete, true), key(egui::Key::Delete, false)],
            vec![key(egui::Key::ArrowLeft, true), ctrl_key(egui::Key::Backspace)],
            vec![key(egui::Key::Home, true), ctrl_key(egui::Key::Home)],
            vec![click(egui::pos2(20.0, 60.0), true), click(egui::pos2(700.0, 200.0), false)],
            vec![egui::Event::Text("overwritten".to_string())],
            vec![egui::Event::Paste(String::new())],
            vec![egui::Event::Ime(egui::ImeEvent::DeleteSurrounding { before_chars: 100, after_chars: 0 })],
            vec![egui::Event::Ime(egui::ImeEvent::Preedit { text: "test".to_string(), active_range_chars: None })],
            vec![egui::Event::Ime(egui::ImeEvent::Commit("done".to_string()))],
            vec![ctrl_key(egui::Key::Z), ctrl_key(egui::Key::Y)],
            vec![key(egui::Key::Tab, true), key(egui::Key::Tab, false)],
        ];

        for events in steps {
            frame(&ctx, &mut text, events);
        }
    }

    fn folded_frame(ctx: &egui::Context, tab: &mut Tab, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                eframe::egui::Pos2::ZERO,
                eframe::egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let overlay = EditorOverlay {
            bracket_guides: true,
            colorize_brackets: true,
        };
        let palette = Palette::dark();
        let output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                show_editor(
                    ui,
                    tab,
                    Theme::GithubDark,
                    "000000",
                    &overlay,
                    &palette,
                    &mut Icons::new(),
                );
            });
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn fold_interactions_do_not_panic() {
        let ctx = egui::Context::default();
        setup(&ctx);

        let mut tab = Tab::new(
            "main.rs",
            "fn outer() {\n    fn inner() {\n        let x = 1;\n        g(x);\n    }\n    let y = 2;\n    h(y);\n}\nfn tail() {\n    step();\n}\n",
            egui_code_editor::Syntax::rust(),
        );

        // Fold `outer` (opening brace on line 0) and `inner` (line 1).
        let reals: Vec<char> = tab.content.chars().collect();
        let (mask, _) = styling::mask_and_links(&tab.content, &tab.syntax);
        let scan = crate::guides::analyze_brackets(&reals, &mask);
        let opens: Vec<usize> = scan
            .brace_pairs
            .iter()
            .filter(|b| b.close != usize::MAX)
            .map(|b| b.open)
            .collect();
        folds::toggle_fold(&mut tab.folds, &opens, &tab.content, 0);
        folds::toggle_fold(&mut tab.folds, &opens, &tab.content, 1);

        // Render with both folds closed.
        folded_frame(&ctx, &mut tab, vec![]);

        let steps: Vec<Vec<egui::Event>> = vec![
            // Type at the caret.
            vec![egui::Event::Text("x".to_string())],
            // Select-all + replace across folded lines.
            vec![ctrl_key(egui::Key::A), egui::Event::Text("fn z() {\n    a();\n}\n".to_string())],
            // Rewrite content fully inside the fold region (past the marker).
            vec![
                ctrl_key(egui::Key::A),
                egui::Event::Paste(
                    "fn outer() {\n    fn inner() {\n        let x = 1;\n    }\n    y();\n}\n"
                        .to_string(),
                ),
            ],
            // Toggle `outer` unfolded again, then re-fold.
            vec![],
        ];
        for events in &steps {
            folded_frame(&ctx, &mut tab, events.clone());
        }
        let reals: Vec<char> = tab.content.chars().collect();
        let (mask, _) = styling::mask_and_links(&tab.content, &tab.syntax);
        let scan = crate::guides::analyze_brackets(&reals, &mask);
        let opens: Vec<usize> = scan
            .brace_pairs
            .iter()
            .filter(|b| b.close != usize::MAX)
            .map(|b| b.open)
            .collect();
        folds::toggle_fold(&mut tab.folds, &opens, &tab.content, 0);
        folded_frame(&ctx, &mut tab, vec![]);

        // Jump to a hidden line (line 3 of real content is inside a fold).
        tab.goto_line = Some(4);
        folded_frame(&ctx, &mut tab, vec![]);

        // Fold everything that remains and render.
        for li in 0..8 {
            folds::toggle_fold(&mut tab.folds, &opens, &tab.content, li);
        }
        folded_frame(&ctx, &mut tab, vec![]);
        for li in 0..8 {
            folds::toggle_fold(&mut tab.folds, &opens, &tab.content, li);
        }
        folded_frame(&ctx, &mut tab, vec![]);
    }
}