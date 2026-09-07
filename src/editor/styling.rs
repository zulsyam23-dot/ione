//! Syntax-coloring pipeline: one pass classifies every char (code / string /
//! comment mask + links), a mask-aware bracket scan runs over the mask, then
//! the colored `LayoutJob` is baked. The mask comes from `lexer` — correct for
//! lifetimes, raw strings, nested comments — and drives everything downstream
//! (diagnostics, folds, completion) on the same truth. Produces `Styled`,
//! consumed by the editor widget right after the text edit is shown.

use eframe::egui::{self, FontId};
use egui::text::LayoutJob;
use egui_code_editor::highlighting::Links;
use egui_code_editor::{ColorTheme, Syntax, Token, TokenType};

use crate::guides::{analyze_brackets, EditorOverlay};
use crate::style::Palette;

use super::lexer;
use super::FONT_SIZE;

/// Per-frame data produced by the layouter for the exact buffer+galley that
/// will be painted (string/comment mask, bracket depth), cached by the
/// text-edit layouter and consumed right after `TextEdit::show`.
#[derive(Clone)]
pub(crate) struct Styled {
    pub(crate) scan: crate::guides::BracketScan,
}

/// One pass per role: char classes + links, a mask-aware bracket scan, then
/// the colored `LayoutJob` (rainbow out of strings/comments). Rust is lexed by
/// `lexer::rust_tokens`; other languages keep the egui tokenizer but their
/// colors are overridden by the correct mask, so a mis-lex can never bleed
/// string color into code or hide a bracket.
pub(crate) fn layout_styled(
    text: &str,
    syntax: &Syntax,
    theme: &ColorTheme,
    overlay: &EditorOverlay,
    palette: &Palette,
) -> (LayoutJob, Links, Styled) {
    let (classes, links) = lexer::classify(text, syntax);
    let mask: Vec<bool> = classes.iter().map(|&c| c != lexer::CODE).collect();

    let chars: Vec<char> = text.chars().collect();
    let scan = analyze_brackets(&chars, &mask);

    let tokens = if syntax.language == "Rust" {
        lexer::rust_tokens(text, syntax)
    } else {
        Token::default().tokens(syntax, text)
    };

    let mut job = LayoutJob::default();
    let mut ci = 0usize; // char index of the current token in text
    for t in tokens {
        let ty = t.ty();
        let n = t.buffer().chars().count();
        let base = theme.type_color(ty);
        let buf = t.buffer();
        let mut start = 0usize; // byte offset of the current color run
        let mut run_color = base;
        for (k, (byte, _)) in buf.char_indices().enumerate() {
            let class = classes.get(ci + k).copied().unwrap_or(lexer::CODE);
            let color = match class {
                lexer::STR => theme.type_color(TokenType::Str('"')),
                lexer::COMMENT => theme.type_color(TokenType::Comment(false)),
                _ if overlay.colorize_brackets => {
                    match scan.depths.get(ci + k).copied().unwrap_or(u8::MAX) {
                        u8::MAX => base,
                        depth => {
                            palette.bracket_rainbow[depth as usize % palette.bracket_rainbow.len()]
                        }
                    }
                }
                _ => base,
            };
            if color != run_color {
                if byte > start {
                    job.append(&buf[start..byte], 0.0, format_font(FONT_SIZE, run_color));
                }
                run_color = color;
                start = byte;
            }
        }
        if buf.len() > start {
            job.append(&buf[start..], 0.0, format_font(FONT_SIZE, run_color));
        }
        ci += n;
    }

    (job, links, Styled { scan })
}

/// String/comment mask + link ranges of arbitrary text — also used on the
/// unfolded (real) content to keep fold regions, guide suppression, and the
/// diagnostics honest about what is inside a string/comment. Mask is `true`
/// for every STR/COMMENT char.
pub(crate) fn mask_and_links(text: &str, syntax: &Syntax) -> (Vec<bool>, Links) {
    let (classes, links) = lexer::classify(text, syntax);
    (classes.iter().map(|&c| c != lexer::CODE).collect(), links)
}

pub(crate) fn format_font(font_size: f32, color: egui::Color32) -> egui::text::TextFormat {
    egui::text::TextFormat::simple(FontId::monospace(font_size), color)
}

/// Style fingerprint for cache keys: everything the colored `LayoutJob` and
/// the mask+links depend on — theme, overlay toggles, rainbow palette, and the
/// syntax language (Save As can swap the language without the content
/// changing). Fold guides are excluded: the guides pass uses its own scan.
pub(crate) fn style_key(
    theme: &ColorTheme,
    overlay: &EditorOverlay,
    palette: &Palette,
    syntax: &Syntax,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    theme.hash(&mut h);
    overlay.colorize_brackets.hash(&mut h);
    for c in palette.bracket_rainbow {
        c.to_array().hash(&mut h);
    }
    syntax.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod styling_tests {
    use super::*;
    use eframe::egui;
    use egui::epaint::text::ByteRangeExt;

    #[test]
    fn rainbow_brackets_colored_by_depth() {
        let syntax = Syntax::rust();
        let theme = ColorTheme::monocolor(true, "000000", "ffffff", "fff", "fff");
        let overlay = EditorOverlay { bracket_guides: false, colorize_brackets: true };
        let palette = Palette::dark();
        let (job, _links, _styled) = layout_styled("a(b(c{d}))", &syntax, &theme, &overlay, &palette);

        let mut colored = Vec::new();
        for sec in &job.sections {
            let txt = sec.byte_range.slice(&job.text);
            if txt.chars().any(|c| "(){}[]".contains(c)) {
                colored.push(sec.format.color);
            }
        }

        // 3 distinct rainbow colors for 3 nesting depths ((/{), and the 'a'
        // non-bracket text stays base white, so no bracket may be plain white.
        colored.sort_by_key(|c| c.to_array());
        colored.dedup();
        assert_eq!(colored.len(), 3);
        assert!(colored.iter().all(|&c| c != egui::Color32::WHITE));
    }
}