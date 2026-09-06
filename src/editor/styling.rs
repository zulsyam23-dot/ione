//! Syntax-coloring pipeline: one lexer pass per role — string/comment mask +
//! links, a mask-aware bracket scan, then the colored `LayoutJob` (rainbow
//! colors, skipped inside strings/comments). Produces `Styled`, consumed by
//! the editor widget right after the text edit is shown.

use eframe::egui::{self, FontId};
use egui::text::LayoutJob;
use egui_code_editor::highlighting::Links;
use egui_code_editor::{ColorTheme, Syntax, Token, TokenType};

use crate::guides::{analyze_brackets, EditorOverlay};
use crate::style::Palette;

use super::FONT_SIZE;

/// Per-frame data produced by the layouter for the exact buffer+galley that
/// will be painted (string/comment mask, bracket depth), cached by the
/// text-edit layouter and consumed right after `TextEdit::show`.
pub(crate) struct Styled {
    pub(crate) scan: crate::guides::BracketScan,
}

/// One lexer pass per role: string/comment mask + links, then a mask-aware
/// bracket scan, then the colored `LayoutJob` (rainbow out of strings/comments).
pub(crate) fn layout_styled(
    text: &str,
    syntax: &Syntax,
    theme: &ColorTheme,
    overlay: &EditorOverlay,
    palette: &Palette,
) -> (LayoutJob, Links, Styled) {
    let (mask, links) = mask_and_links(text, syntax);

    let chars: Vec<char> = text.chars().collect();
    let scan = analyze_brackets(&chars, &mask);

    let mut job = LayoutJob::default();
    let mut token = Token::default();
    let mut ci = 0usize;
    for t in token.tokens(syntax, text) {
        let ty = t.ty();
        let n = t.buffer().chars().count();
        let base = theme.type_color(ty);
        if overlay.colorize_brackets && !matches!(ty, TokenType::Str(_) | TokenType::Comment(_)) {
            let buf = t.buffer();
            let mut start = 0usize; // byte offset of the current color run
            let mut run_color = base;
            for (k, (byte, ch)) in buf.char_indices().enumerate() {
                let color = match scan.depths.get(ci + k).copied().unwrap_or(u8::MAX) {
                    u8::MAX => base,
                    depth => palette.bracket_rainbow[depth as usize % palette.bracket_rainbow.len()],
                };
                if color != run_color {
                    if byte > start {
                        job.append(&buf[start..byte], 0.0, format_font(FONT_SIZE, run_color));
                    }
                    run_color = color;
                    start = byte;
                }
                let _ = ch;
            }
            if buf.len() > start {
                job.append(&buf[start..], 0.0, format_font(FONT_SIZE, run_color));
            }
        } else {
            job.append(t.buffer(), 0.0, format_font(FONT_SIZE, base));
        }
        ci += n;
    }

    (job, links, Styled { scan })
}

/// Lexer pass for string/comment masking + link ranges of arbitrary text —
/// also used on the unfolded (real) content to keep fold regions and guide
/// suppression honest about what is inside a string/comment.
pub(crate) fn mask_and_links(text: &str, syntax: &Syntax) -> (Vec<bool>, Links) {
    let chars: Vec<char> = text.chars().collect();
    let mut mask = vec![false; chars.len()];
    let mut links = Links::default();

    let mut token = Token::default();
    let mut ci = 0usize;
    for t in token.tokens(syntax, text) {
        let n = t.buffer().chars().count();
        if t.ty() == TokenType::Hyperlink {
            links.push(ci..ci + n);
        }
        if matches!(t.ty(), TokenType::Str(_) | TokenType::Comment(_)) {
            for m in mask.iter_mut().skip(ci).take(n) {
                *m = true;
            }
        }
        ci += n;
    }
    (mask, links)
}

pub(crate) fn format_font(font_size: f32, color: egui::Color32) -> egui::text::TextFormat {
    egui::text::TextFormat::simple(FontId::monospace(font_size), color)
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