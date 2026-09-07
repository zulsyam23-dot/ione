//! Lightweight autocomplete / intellisense for the custom editor. Suggestions
//! come from three sources, in presentation order:
//!   1. the file's `Syntax` — keywords, types, special forms (true/null/None…);
//!   2. symbols extracted from the file by `outline` (functions/structs/classes)
//!      with their parameter groups, giving a lightweight intellisense feel;
//!   3. identifiers already used in the file, via the same lexer the syntax
//!      highlighter uses.
//! True semantic analysis (variable types, documentation, LSP) is out of scope.

use std::collections::BTreeSet;

use egui_code_editor::{Syntax, Token, TokenType};

use crate::outline::Symbol;

/// Popup coloring bucket, mirroring `TokenType` for keyword/type/special.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Keyword,
    Type,
    Special,
    Func,
    Word,
}

/// One suggestion row.
#[derive(Clone, Debug)]
pub struct Item {
    /// Full word to insert (rendered as `prefix + tail`).
    pub text: String,
    /// Just the part past the typed prefix — pasting this completes the word.
    pub tail: String,
    /// Right-aligned metadata: `fn add(a: i32, b: i32) · 12` or the kind name.
    pub detail: String,
    pub kind: Kind,
}

/// Live per-tab popup state; `None` while hidden.
pub struct CompletionState {
    pub items: Vec<Item>,
    pub selected: usize,
    /// Typed prefix these `items` were built from; a change rebuilds the list.
    pub prefix: String,
}

/// A key press the open popup handles itself instead of passing to TextEdit.
pub enum Action {
    /// Move the highlight up/down (ArrowUp / ArrowDown).
    Prev,
    Next,
    /// Insert the selected item (Enter / Tab).
    Accept,
    /// Hide the popup (Esc, click, caret move).
    Close,
}

/// Identifier characters a completion prefix can span.
pub(crate) fn is_word_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Character that should sit right after the cursor for the popup to appear:
/// the cursor must be at the end of an identifier run (or at end of line).
pub fn next_char_allows(next: Option<char>) -> bool {
    next.is_none_or(|c| !is_word_char(c))
}

/// The word being typed at the end of `line` up to `col_char` (char counts).
/// Returns the trailing identifier run, e.g. "pri" in `let x = pri|`.
pub fn prefix_at(line: &str, col_char: usize) -> String {
    let mut s = String::with_capacity(col_char);
    for c in line.chars().take(col_char) {
        if is_word_char(c) {
            s.push(c);
        } else {
            s.clear();
        }
    }
    s
}

/// The identifier being typed before `cursor_char` (display char index) plus
/// the char sitting under the cursor — all in display (folded) coordinates.
pub fn prefix_at_cursor(text: &str, cursor_char: usize) -> (String, Option<char>) {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let cur = cursor_char.min(n);
    let line_start = chars[..cur].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
    let line_end = chars[cur..].iter().position(|&c| c == '\n').map_or(n, |i| cur + i);
    let line: String = chars[line_start..line_end].iter().collect();
    let prefix = prefix_at(&line, cur - line_start);
    let next = chars.get(cur).copied();
    (prefix, next)
}

/// Assemble suggestions matching `prefix`, in the source order above.
pub fn build_items(content: &str, syntax: &Syntax, symbols: &[Symbol], prefix: &str) -> Vec<Item> {
    if prefix.is_empty() {
        return Vec::new();
    }
    let lower = prefix.to_lowercase();
    let pc = prefix.chars().count();
    let full_match = |word: &str| {
        word.chars().count() >= pc
            && word.chars().take(pc).collect::<String>().to_lowercase() == lower
    };

    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |items: &mut Vec<Item>, word: String, detail: String, kind: Kind| {
        if !seen.insert(word.clone()) {
            return;
        }
        let tail = word.chars().skip(pc).collect();
        items.push(Item {
            text: word,
            tail,
            detail,
            kind,
        });
    };

    for kw in &syntax.keywords {
        if full_match(kw) {
            push(&mut items, (*kw).to_string(), String::new(), Kind::Keyword);
        }
    }
    for ty in &syntax.types {
        if full_match(ty) {
            push(&mut items, (*ty).to_string(), String::new(), Kind::Type);
        }
    }
    for sp in &syntax.special {
        if full_match(sp) {
            push(&mut items, (*sp).to_string(), String::new(), Kind::Special);
        }
    }
    for sym in symbols {
        if full_match(&sym.name) {
            let detail = if sym.params.is_empty() {
                format!("{} ·:{}", sym.kind, sym.line)
            } else {
                format!("{} {} ·:{}", sym.kind, sym.params, sym.line)
            };
            push(&mut items, sym.name.clone(), detail, Kind::Func);
        }
    }

    // File identifiers: the highlighter's own token stream, deduped.
    let mut words = BTreeSet::new();
    for t in Token::default().tokens(syntax, content) {
        if matches!(t.ty(), TokenType::Literal | TokenType::Function) && !t.buffer().is_empty() {
            let buf = t.buffer();
            if buf.chars().all(is_word_char) {
                words.insert(buf.to_string());
            }
        }
    }
    for word in words {
        if full_match(&word) {
            push(&mut items, word, String::new(), Kind::Word);
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust() -> Syntax {
        Syntax::rust()
    }

    #[test]
    fn syntax_words_and_symbols_are_merged_in_order() {
        let content = "fn main() { ary_other(); }\nfn add(a: i32, b: i32) {\n}\n";
        let syms = crate::outline::extract_symbols(content, &rust());
        let items = build_items(content, &rust(), &syms, "a");

        assert!(items.iter().any(|i| i.text == "add" && i.kind == Kind::Func));
        assert!(items.iter().any(|i| i.text == "ary_other" && i.kind == Kind::Word));
        assert!(items.iter().any(|i| i.text == "as" && i.kind == Kind::Keyword));
        assert!(items.iter().any(|i| i.text == "Arc" && i.kind == Kind::Type));
        // The function's signature detail carries its parameter group.
        let add = items.iter().find(|i| i.text == "add").unwrap();
        assert_eq!(add.detail, "fn (a: i32, b: i32) ·:2");
        // Dedup: "add" must not appear twice despite being a symbol + identifier.
        let count = items.iter().filter(|i| i.text == "add").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn tail_and_prefix_filtering() {
        let content = "let mut counter = 0;\ncounter = counter + 1;\n";
        let items = build_items(content, &rust(), &[], "count");
        assert!(items.iter().any(|i| i.text == "counter"));
        let counter = items.iter().find(|i| i.text == "counter").unwrap();
        assert_eq!(counter.tail, "er"); // "count" already typed
        // No match for a prefix no identifier shares.
        let none = build_items(content, &rust(), &[], "zzz");
        assert!(none.is_empty());
    }

    #[test]
    fn prefix_at_takes_only_the_trailing_identifier_run() {
        assert_eq!(prefix_at("let counter = 0", 11), "counter");
        assert_eq!(prefix_at("some_fn(", 7), "some_fn");
        assert_eq!(prefix_at("  pri", 5), "pri");
        assert_eq!(prefix_at("x + ", 4), "");
    }

    #[test]
    fn prefix_at_cursor_spans_lines_and_reports_following_char() {
        // Cursor after "cou" in "counter" on line 2.
        let text = "fn main() {\n    let counter = 0;\n}\n";
        let (prefix, next) = prefix_at_cursor(text, 23);
        assert_eq!(prefix, "cou");
        assert_eq!(next, Some('n'));
        // Cursor at end of line → prefix is the word, next is None.
        let (prefix, next) = prefix_at_cursor("let x = count", 13);
        assert_eq!(prefix, "count");
        assert_eq!(next, None);
        // Cursor right after an operator → no prefix.
        let (prefix, _) = prefix_at_cursor("let total = a + ", 16);
        assert_eq!(prefix, "");
    }
}