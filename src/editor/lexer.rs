//! Correct string/comment scanning and Rust-aware tokenization — the single
//! source of truth for what is "inside a string" vs "inside a comment" vs
//! code. Everything downstream (diagnostics squiggles, fold targets, bracket
//! guides/rainbow, completion context, layout colors) consumes this mask, so
//! a mistake here mis-fires them all.
//!
//! Why this exists: the `egui_code_editor` lexer is a per-char automaton with
//! no knowledge of Rust lifetimes (`'a`), labels (`'label:`), raw strings
//! (`r"…"`, `br#"…"#`), nested block comments, or multi-line `"""` strings.
//! The instant one of those appears it swallows the rest of the buffer as a
//! string, so later brackets are skipped — false "unclosed bracket" errors
//! pile up and real ones are missed.

use egui_code_editor::highlighting::Links;
use egui_code_editor::{Syntax, Token, TokenType};

/// Per-char class. `STR`/`COMMENT` are the "masked" bits the bracket scan
/// must skip; `STR` vs `COMMENT` also picks the paint color.
pub(crate) const CODE: u8 = 0;
pub(crate) const STR: u8 = 1;
pub(crate) const COMMENT: u8 = 2;

/// One pass over `text`: per-char class mask + hyperlink ranges (links still
/// scraped from the egui lexer, which is fine for URLs — purely cosmetic).
pub(crate) fn classify(text: &str, syntax: &Syntax) -> (Vec<u8>, Links) {
    let chars: Vec<char> = text.chars().collect();
    let mut cls = vec![CODE; chars.len()];
    match syntax.language {
        "Rust" => classify_rust(&mut cls, &chars),
        "js" | "typescript" => classify_js(&mut cls, &chars),
        _ => classify_generic(&mut cls, &chars, syntax),
    }
    (cls, links(text, syntax))
}

fn links(text: &str, syntax: &Syntax) -> Links {
    let mut out = Links::default();
    let mut ci = 0usize;
    for t in Token::default().tokens(syntax, text) {
        let n = t.buffer().chars().count();
        if t.ty() == TokenType::Hyperlink {
            out.push(ci..ci + n);
        }
        ci += n;
    }
    out
}

/// Token stream for Rust: correct lifetimes, raw strings, byte strings, char
/// escapes and nested block comments, classified with `syntax` color rules.
pub(crate) fn rust_tokens(text: &str, syntax: &Syntax) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < n {
        let c = chars[i];
        let seg = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
        if c == '/' {
            if i + 1 < n && chars[i + 1] == '/' {
                let e = line_comment_end(&chars, i).unwrap_or(n);
                out.push(Token::new(TokenType::Comment(false), seg(i, e)));
                i = e;
                continue;
            }
            if i + 1 < n && chars[i + 1] == '*' {
                let e = block_comment_end(&chars, i).unwrap_or(n);
                out.push(Token::new(TokenType::Comment(true), seg(i, e)));
                i = e;
                continue;
            }
            out.push(Token::new(TokenType::Punctuation('/'), seg(i, i + 1)));
            i += 1;
            continue;
        }
        if c == 'r' {
            if let Some(h) = raw_open(&chars, i) {
                let e = raw_end(&chars, i, h).unwrap_or(n);
                out.push(Token::new(TokenType::Str('"'), seg(i, e)));
                i = e;
                continue;
            }
        }
        if c == 'b' && i + 1 < n && chars[i + 1] == 'r' {
            if let Some(h) = raw_open(&chars, i + 1) {
                let e = raw_end(&chars, i + 1, h).unwrap_or(n);
                out.push(Token::new(TokenType::Str('"'), seg(i, e)));
                i = e;
                continue;
            }
        }
        if c == '"' {
            let e = string_end(&chars, i).unwrap_or(n);
            out.push(Token::new(TokenType::Str('"'), seg(i, e)));
            i = e;
            continue;
        }
        if c == 'b' && i + 1 < n {
            if chars[i + 1] == '\'' {
                if let Some(e) = char_literal_end(&chars, i + 1) {
                    out.push(Token::new(TokenType::Str('\''), seg(i, e)));
                    i = e;
                    continue;
                }
            }
            if chars[i + 1] == '"' {
                let e = string_end(&chars, i + 1).unwrap_or(n);
                out.push(Token::new(TokenType::Str('"'), seg(i, e)));
                i = e;
                continue;
            }
        }
        if c == '\'' {
            if let Some(e) = char_literal_end(&chars, i) {
                out.push(Token::new(TokenType::Str('\''), seg(i, e)));
                i = e;
            } else {
                // Lifetime `'a` / label `'name:`, or a stray quote.
                let mut e = i + 1;
                while e < n && (chars[e].is_alphanumeric() || chars[e] == '_') {
                    e += 1;
                }
                out.push(Token::new(TokenType::Literal, seg(i, e)));
                i = e;
            }
            continue;
        }
        if c.is_numeric() {
            i = number_token(&chars, i, &mut out);
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word = seg(start, i);
            let ty = if syntax.is_keyword(&word) {
                TokenType::Keyword
            } else if syntax.is_type(&word) {
                TokenType::Type
            } else if syntax.is_special(&word) {
                TokenType::Special
            } else {
                TokenType::Literal
            };
            let ty = if i < n && chars[i] == '(' {
                TokenType::Function
            } else {
                ty
            };
            out.push(Token::new(ty, word));
            continue;
        }
        if c.is_whitespace() {
            let start = i;
            while i < n && chars[i].is_whitespace() {
                i += 1;
            }
            out.push(Token::new(TokenType::Whitespace(c), seg(start, i)));
            continue;
        }
        let ty = if c.is_ascii_punctuation() {
            TokenType::Punctuation(c)
        } else {
            TokenType::Unknown
        };
        out.push(Token::new(ty, seg(i, i + 1)));
        i += 1;
    }
    out
}

/// Numeric literal (dec/hex/bin/oct, `_` separators, `.` float, letter suffix).
/// Returns the end index, pushing one token.
fn number_token(chars: &[char], i: usize, out: &mut Vec<Token>) -> usize {
    let n = chars.len();
    let start = i;
    let mut i = i;
    let ty = if i + 1 < n && matches!(chars[i + 1], 'x' | 'X' | 'b' | 'B' | 'o' | 'O') {
        i += 2;
        while i < n && (chars[i].is_ascii_hexdigit() || chars[i] == '_') {
            i += 1;
        }
        TokenType::Numeric(false)
    } else {
        while i < n && (chars[i].is_numeric() || chars[i] == '_') {
            i += 1;
        }
        let mut float = false;
        if i + 1 < n && chars[i] == '.' && chars[i + 1].is_numeric() {
            float = true;
            i += 1;
            while i < n && (chars[i].is_numeric() || chars[i] == '_') {
                i += 1;
            }
        }
        while i < n && (chars[i].is_ascii_alphanumeric()) {
            i += 1;
        }
        TokenType::Numeric(float)
    };
    let s: String = chars[start..i].iter().collect();
    out.push(Token::new(ty, s));
    i
}

fn line_comment_end(chars: &[char], start: usize) -> Option<usize> {
    chars[start + 2..].iter().position(|&c| c == '\n').map(|k| start + 2 + k)
}

/// End of a nested `/* … */` region (Rust nests block comments).
fn block_comment_end(chars: &[char], start: usize) -> Option<usize> {
    let n = chars.len();
    let mut depth = 1usize;
    let mut j = start + 2;
    while j < n {
        if chars[j] == '/' && j + 1 < n && chars[j + 1] == '*' {
            depth += 1;
            j += 2;
        } else if chars[j] == '*' && j + 1 < n && chars[j + 1] == '/' {
            depth -= 1;
            j += 2;
            if depth == 0 {
                return Some(j);
            }
        } else {
            j += 1;
        }
    }
    None
}

/// End (exclusive) of a `"…"` string starting at `qi`, honoring `\\` escapes
/// and `\`+newline continuations. `None` = unterminated (mask to EOF).
fn string_end(chars: &[char], qi: usize) -> Option<usize> {
    let n = chars.len();
    let q = chars[qi];
    let mut j = qi + 1;
    while j < n {
        match chars[j] {
            '\\' => {
                j += 1;
                if j >= n {
                    return None;
                }
                if chars[j] == '\n' {
                    j += 1;
                } else {
                    j += 1;
                }
            }
            ch if ch == q => return Some(j + 1),
            _ => j += 1,
        }
    }
    None
}

/// If `chars[i]` starts a Rust char literal (incl. escapes, `b'…'` via prefix
/// removal) return its end; otherwise `None` — the quote is a lifetime/label.
fn char_literal_end(chars: &[char], qi: usize) -> Option<usize> {
    let n = chars.len();
    let mut j = qi + 1;
    if j >= n {
        return None;
    }
    let c1 = chars[j];
    if c1 == '\'' {
        return Some(j + 1); // empty `''` (invalid but harmless)
    }
    if c1 == '\\' {
        j += 1;
        if j < n && chars[j] == 'u' {
            j += 1;
            if j < n && chars[j] == '{' {
                while j < n && chars[j] != '}' {
                    j += 1;
                }
                if j < n {
                    j += 1;
                }
            }
        } else if j < n {
            j += 1;
        }
        return if j < n && chars[j] == '\'' { Some(j + 1) } else { None };
    }
    if j + 1 < n && chars[j + 1] == '\'' {
        Some(j + 2)
    } else {
        None
    }
}

/// Raw string opener at `ri` (`r`/`br` position): optional `#`s then `"`.
/// Returns the hash count.
fn raw_open(chars: &[char], ri: usize) -> Option<usize> {
    let n = chars.len();
    let mut j = ri + 1;
    let mut hashes = 0usize;
    while j < n && chars[j] == '#' {
        hashes += 1;
        j += 1;
    }
    if j < n && chars[j] == '"' {
        Some(hashes)
    } else {
        None
    }
}

/// End (exclusive) of a raw string at `ri`, closed by `"` + exactly `hashes`
/// `#`. `None` = unterminated.
fn raw_end(chars: &[char], ri: usize, hashes: usize) -> Option<usize> {
    let n = chars.len();
    let mut j = ri + 1;
    while j < n && chars[j] == '#' {
        j += 1;
    }
    if j >= n || chars[j] != '"' {
        return None;
    }
    j += 1; // after the opening quote
    while j < n {
        if chars[j] == '"' {
            let mut k = 0usize;
            while j + 1 + k < n && chars[j + 1 + k] == '#' {
                k += 1;
            }
            if k >= hashes {
                return Some(j + 1 + hashes);
            }
        }
        j += 1;
    }
    None
}

fn mark(cls: &mut [u8], range: std::ops::Range<usize>, k: u8) {
    for c in cls[range].iter_mut() {
        *c = k;
    }
}

fn classify_rust(cls: &mut [u8], chars: &[char]) {
    let n = chars.len();
    let mut i = 0usize;
    while i < n {
        if chars[i] == '/' {
            if i + 1 < n && chars[i + 1] == '/' {
                let e = line_comment_end(chars, i).unwrap_or(n);
                mark(cls, i..e, COMMENT);
                i = e;
                continue;
            }
            if i + 1 < n && chars[i + 1] == '*' {
                let e = block_comment_end(chars, i).unwrap_or(n);
                mark(cls, i..e, COMMENT);
                i = e;
                continue;
            }
        }
        if chars[i] == 'r' && let Some(h) = raw_open(chars, i) {
            let e = raw_end(chars, i, h).unwrap_or(n);
            mark(cls, i..e, STR);
            i = e;
            continue;
        }
        if chars[i] == 'b' && i + 1 < n {
            if chars[i + 1] == 'r' && let Some(h) = raw_open(chars, i + 1) {
                let e = raw_end(chars, i + 1, h).unwrap_or(n);
                mark(cls, i..e, STR);
                i = e;
                continue;
            }
            if chars[i + 1] == '\'' && let Some(e) = char_literal_end(chars, i + 1) {
                mark(cls, i..e, STR);
                i = e;
                continue;
            }
            if chars[i + 1] == '"' {
                let e = string_end(chars, i + 1).unwrap_or(n);
                mark(cls, i..e, STR);
                i = e;
                continue;
            }
        }
        if chars[i] == '"' {
            let e = string_end(chars, i).unwrap_or(n);
            mark(cls, i..e, STR);
            i = e;
            continue;
        }
        if chars[i] == '\'' {
            if let Some(e) = char_literal_end(chars, i) {
                mark(cls, i..e, STR);
                i = e;
            } else {
                i += 1; // lifetime / label stays code
            }
            continue;
        }
        i += 1;
    }
}

fn has(chars: &[char], i: usize, s: &str) -> bool {
    let s: Vec<char> = s.chars().collect();
    chars.get(i..i + s.len()) == Some(&s[..])
}

/// Generic language scanner driven by `syntax` (line/block comments, quotes,
/// plus `'''`/`"""` multi-line strings for quote-languages).
fn classify_generic(cls: &mut [u8], chars: &[char], syntax: &Syntax) {
    let n = chars.len();
    let cmt0 = syntax.comment_multiline[0];
    let cmt1 = syntax.comment_multiline[1];
    let cm = syntax.comment;
    let mut i = 0usize;
    while i < n {
        let c = chars[i];
        if !cmt0.is_empty() && has(chars, i, cmt0) {
            let e = if !cmt1.is_empty() {
                chars[i + cmt0.chars().count()..]
                    .windows(cmt1.chars().count())
                    .position(|w| w.iter().collect::<String>() == cmt1)
                    .map(|k| i + cmt0.chars().count() + k + cmt1.chars().count())
                    .unwrap_or(n)
            } else {
                n
            };
            mark(cls, i..e, COMMENT);
            i = e;
            continue;
        }
        if !cm.is_empty() && has(chars, i, cm) {
            let from = i + cm.chars().count();
            let e = chars[from..]
                .iter()
                .position(|&d| d == '\n')
                .map(|k| from + k)
                .unwrap_or(n);
            mark(cls, i..e, COMMENT);
            i = e;
            continue;
        }
        if syntax.quotes.contains(&c) {
            let mut j = i;
            while j < n && chars[j] == c {
                j += 1;
            }
            if j - i >= 3 {
                // Triple-quoted multi-line string.
                let seq = cmt0.chars().count().max(3);
                let close = chars[j..]
                    .windows(seq)
                    .position(|w| w.iter().all(|&d| d == c))
                    .map(|k| j + k + seq);
                let e = close.unwrap_or(n);
                mark(cls, i..e, STR);
                i = e;
                continue;
            }
            let e = string_end(chars, i).unwrap_or(n);
            mark(cls, i..e, STR);
            i = e;
            continue;
        }
        i += 1;
    }
}

// ---------------------------------------------------------------------------
// JS / TS / JSX / TSX
// ---------------------------------------------------------------------------

/// Keywords after which a `/` may begin a regex literal (they are followed by an
/// expression, not an operand).
const JS_EXPR_PREFIX: [&str; 14] = [
    "return", "typeof", "instanceof", "in", "of", "new", "delete", "void", "case",
    "throw", "do", "else", "yield", "await",
];

/// JS/TS scanner. The two traps the generic scanner falls into:
///   * template literals `` `…${code}…` `` — the `${ … }` body is real code and
///     its braces/brackets must be scanned, not masked with the rest of the
///     template;
///   * regex literals `/…/` — their quantifiers `{2,3}`, groups `( … )` and
///     char classes `[ … ]` are not code brackets, but only when the `/` really
///     starts a regex (not integer division).
fn classify_js(cls: &mut [u8], chars: &[char]) {
    js_scan_code(cls, chars, 0, false);
}

/// Scan code from `i`. With `stop_at_close_brace` (a `${…}` interpolation) the
/// first `}` at brace depth 0 is the template delimiter and is marked `STR`.
/// Returns the index just past the scanned region.
fn js_scan_code(cls: &mut [u8], chars: &[char], mut i: usize, stop_at_close_brace: bool) -> usize {
    let n = chars.len();
    let mut depth = 0usize; // brace depth inside an interpolation
    let mut expect_regex = true; // statement start ⇒ `/` is a regex, not division
    while i < n {
        if has(chars, i, "//") {
            let e = line_comment_end(chars, i).unwrap_or(n);
            mark(cls, i..e, COMMENT);
            i = e;
            expect_regex = true;
            continue;
        }
        if has(chars, i, "/*") {
            let e = chars[i + 2..].windows(2).position(|w| w == ['*', '/'])
                .map(|k| i + 2 + k + 2)
                .unwrap_or(n);
            mark(cls, i..e, COMMENT);
            i = e;
            expect_regex = true;
            continue;
        }
        let c = chars[i];
        if c == '\'' || c == '"' {
            let e = string_end(chars, i).unwrap_or(n);
            mark(cls, i..e, STR);
            i = e;
            expect_regex = false;
            continue;
        }
        if c == '`' {
            i = js_template(cls, chars, i);
            expect_regex = false;
            continue;
        }
        if c == '/' {
            // JSX closing/self-closing tag (`</div>`, `<br />`) — never a regex.
            let after_tag = i > 0 && chars[i - 1] == '<';
            if expect_regex && !after_tag && let Some(e) = js_regex_end(chars, i) {
                mark(cls, i..e, STR);
                i = e;
                expect_regex = false;
                continue;
            }
            i += 1; // division
            expect_regex = false;
            continue;
        }
        if c == '{' {
            if stop_at_close_brace {
                depth += 1;
            }
            i += 1;
            expect_regex = true;
            continue;
        }
        if c == '}' {
            if stop_at_close_brace {
                if depth == 0 {
                    mark(cls, i..i + 1, STR); // `${…}` delimiter
                    return i + 1;
                }
                depth -= 1;
            }
            i += 1;
            expect_regex = false;
            continue;
        }
        if c == ')' || c == ']' {
            i += 1;
            expect_regex = false;
            continue;
        }
        if c.is_numeric() {
            i = js_number_end(chars, i);
            expect_regex = false;
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == '$' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            expect_regex = JS_EXPR_PREFIX.contains(&word.as_str());
            continue;
        }
        if c == '+' || c == '-' {
            // `++` / `--` postfix keeps the previous operand: `i++ / 2` is
            // division, so the `/` must not be treated as a regex.
            if i > 1 && chars[i - 1] == c && (chars[i - 2].is_alphanumeric() || chars[i - 2] == ')') {
                i += 1;
                expect_regex = false;
                continue;
            }
        }
        i += 1;
        expect_regex = true; // operators / `(` `[` `,` `;` `:` `=` `!` `?` `&` `|` …
    }
    n
}

/// Template literal starting at the backtick `` ` ``: text is `STR`, and each
/// `${ … }` interpolation is rescanned as code. Returns the index past the
/// closing backtick (or EOF for an unterminated template).
fn js_template(cls: &mut [u8], chars: &[char], start: usize) -> usize {
    let n = chars.len();
    let mut i = start;
    mark(cls, i..i + 1, STR);
    i += 1;
    while i < n {
        let c = chars[i];
        if c == '\\' {
            let e = (i + 2).min(n);
            mark(cls, i..e, STR);
            i = e;
            continue;
        }
        if c == '`' {
            mark(cls, i..i + 1, STR);
            return i + 1;
        }
        if c == '$' && has(chars, i, "${") {
            mark(cls, i..i + 2, STR); // `$` and `{` are template delimiters
            i = js_scan_code(cls, chars, i + 2, true);
            continue;
        }
        mark(cls, i..i + 1, STR);
        i += 1;
    }
    n
}

/// End (exclusive) of a JS/TS regex literal at `/`, honoring `\` escapes, char
/// classes `[ … ]` and (optionally) flags. `None` if no closing `/` before a
/// newline — a regex can't span lines, so the `/` was actually division.
fn js_regex_end(chars: &[char], start: usize) -> Option<usize> {
    let n = chars.len();
    let mut j = start + 1;
    let mut in_class = false;
    while j < n {
        match chars[j] {
            '\\' => j += 2,
            '\n' | '\r' => return None,
            '[' => {
                in_class = true;
                j += 1;
            }
            ']' => {
                in_class = false;
                j += 1;
            }
            '/' if !in_class => {
                j += 1;
                while j < n && chars[j].is_ascii_alphabetic() {
                    j += 1;
                }
                return Some(j);
            }
            _ => j += 1,
        }
    }
    None
}

fn js_number_end(chars: &[char], start: usize) -> usize {
    let mut j = start;
    while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_' || chars[j] == '.') {
        j += 1;
    }
    j
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask_errs(content: &str, syntax: &Syntax) -> bool {
        let (cls, _) = classify(content, syntax);
        let mask: Vec<bool> = cls.iter().map(|&c| c != CODE).collect();
        let chars: Vec<char> = content.chars().collect();
        let scan = crate::guides::analyze_brackets(&chars, &mask);
        scan.pairs.iter().any(|p| p.close == usize::MAX) || !scan.unmatched_closes.is_empty()
    }

    #[test]
    fn lifetime_never_masks_the_rest_of_the_file() {
        let rust = Syntax::rust();
        let src = "fn f<'a>(x: &'a str) -> &'a str { x }\nfn g() { let y = (1); }\n";
        let (cls, _) = classify(src, &rust);
        // No char outside a genuine string may be marked STR (the old lexer
        // turned everything after `'a` into a string).
        assert!(cls.iter().all(|&c| c == CODE));
    }

    #[test]
    fn raw_string_brackets_are_ignored() {
        let src = "fn main() {\n    let s = r#\"{ not real }\"#;\n    if a { b(); }\n}\n";
        assert!(!mask_errs(src, &Syntax::rust()));
    }

    #[test]
    fn char_literals_and_escapes_are_masked() {
        for src in [
            "fn f() { let c = '{'; let d = '}'; }\n",
            "fn f() { let c = '\\''; }\n",
            "fn f() { let c = '\\u{7D}'; }\n",
            "fn f() { let c = b'['; let d = b'\"'; }\n",
        ] {
            assert!(!mask_errs(src, &Syntax::rust()), "unexpected errors in {src}");
        }
    }

    #[test]
    fn nested_block_comments_are_ignored() {
        let src = "fn f() {\n    /* outer /* { still comment */ */\n    let x = 1;\n}\n";
        assert!(!mask_errs(src, &Syntax::rust()));
    }

    #[test]
    fn python_triple_quotes_are_ignored() {
        let src = "def f():\n    ''' doc { } \"\" } '''\n    return (1)\n";
        assert!(!mask_errs(src, &Syntax::python()));
        let src = "s = \"\"\"multi { not code }\"\"\"\nx = (1)\n";
        assert!(!mask_errs(src, &Syntax::python()));
    }

    #[test]
    fn genuine_unbalance_after_lifetime_still_detected() {
        let src = "fn f<'a>(x: &'a str) -> &'a str {\n    g((x);\n}\n";
        assert!(mask_errs(src, &Syntax::rust()));
    }

    #[test]
    fn rust_tokens_keep_lifetimes_and_strings_sane() {
        let toks = rust_tokens("fn f<'a>(x: &'a str) { let s = \"hi\"; g(1); }", &Syntax::rust());
        let strs: Vec<usize> = toks
            .iter()
            .filter(|t| matches!(t.ty(), TokenType::Str(_)))
            .map(|t| t.buffer().len())
            .collect();
        assert_eq!(strs, vec![4]); // only the "hi" string
        assert!(toks.iter().any(|t| t.ty() == TokenType::Keyword));
        assert!(toks.iter().any(|t| t.ty() == TokenType::Function));
    }

    fn js() -> Syntax {
        Syntax::new("js")
            .with_comment("//")
            .with_comment_multiline(["/*", "*/"])
    }

    #[test]
    fn js_template_interpolation_is_code() {
        let src = "const s = `a ${b({c: 1})} d ${e[0] + f.g()} h`; let x = 1;\n";
        assert!(!mask_errs(src, &js()));
        // An unbalanced bracket inside `${…}` must still be flagged.
        let src = "const s = `a ${foo(1,2} b`; let y = 2;\n";
        assert!(mask_errs(src, &js()), "unclosed paren inside interpolation missed");
    }

    #[test]
    fn js_regex_quantifiers_and_classes_are_not_code() {
        let src = "const re1 = /}/;\nconst re2 = /a{2,3}/;\nconst re3 = /[{}(]/;\nlet ok = 1;\n";
        assert!(!mask_errs(src, &js()));
    }

    #[test]
    fn js_division_is_not_a_regex() {
        let src = "const d = a / b;\nconst e = x[0] / y;\nconst f = i++ / 2;\nlet ok = 1;\n";
        assert!(!mask_errs(src, &js()));
    }

    #[test]
    fn js_regex_after_return_keyword() {
        let src = "function f(v) { return /x/; }\nlet ok = 1;\n";
        assert!(!mask_errs(src, &js()));
    }

    #[test]
    fn jsx_closing_tags_are_not_regex() {
        let src = "const el = <div><span>{a.b(c)}</span><img src=\"x\" /></div>;\n";
        assert!(!mask_errs(src, &js()));
    }

    #[test]
    fn ts_generic_arrow_is_clean() {
        let src = "const f = <T,>(x: T) => x;\nlet ok = 1;\n";
        assert!(!mask_errs(src, &js()));
    }

    #[test]
    fn js_genuine_unbalance_still_detected() {
        let src = "function f() { const x = (1; }\n";
        assert!(mask_errs(src, &js()));
    }
}