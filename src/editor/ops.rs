//! Pure whole-line text operations on the real buffer, in char coordinates
//! (matching `editor::multi` / `folds`, which also edit real text). Used by
//! the editor's keyboard line commands: toggle line comment (`Ctrl+/`),
//! duplicate (`Ctrl+Shift+D`), delete (`Ctrl+Shift+K`), move (`Alt+↑/↓`).

/// Char index of every line start in `content`.
fn line_starts(content: &str) -> Vec<usize> {
    let mut out = vec![0];
    for (i, c) in content.chars().enumerate() {
        if c == '\n' {
            out.push(i + 1);
        }
    }
    out
}

/// `(start, end)` char span covering the lines of `[s, e)` — zero-width when
/// the caret has no selection. `end` sits just past the block's trailing
/// newline (or EOF), so delete/duplicate/move swap whole lines in one span.
pub fn lines_span(content: &str, s: usize, e: usize) -> (usize, usize) {
    let total = content.chars().count();
    let starts = line_starts(content);
    let line = |ci: usize| starts.partition_point(|&x| x <= ci).saturating_sub(1);
    let l = line(s.min(total));
    let r = if e <= s { l } else { line((e - 1).min(total)) };
    (starts[l], starts.get(r + 1).copied().unwrap_or(total))
}

/// Toggle a `// ` line comment on every non-blank line inside `[s, e)`.
/// Blank lines are left untouched.
pub fn toggle_line_comment(content: &mut String, s: usize, e: usize) {
    let (ss, se) = lines_span(content, s, e);
    let chars: Vec<char> = content.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    out.extend_from_slice(&chars[..ss]);
    let mut i = ss;
    while i < se {
        let mut j = i;
        while j < se && chars[j] != '\n' {
            j += 1;
        }
        let ws = chars[i..j]
            .iter()
            .take_while(|&&c| c == ' ' || c == '\t')
            .count();
        if ws < j - i {
            let a = i + ws;
            let commented = chars.get(a) == Some(&'/') && chars.get(a + 1) == Some(&'/');
            if commented {
                // Drop `//` and its separator space (inserted by the "// "
                // form), so the round trip restores the exact indentation.
                let n = if chars.get(a + 2) == Some(&' ') { 3 } else { 2 };
                out.extend_from_slice(&chars[i..a]);
                out.extend_from_slice(&chars[a + n..j]);
            } else {
                out.extend_from_slice(&chars[i..a]);
                out.extend_from_slice(&['/', '/', ' ']);
                out.extend_from_slice(&chars[a..j]);
            }
        } else {
            out.extend_from_slice(&chars[i..j]);
        }
        if j < se {
            out.push('\n');
        }
        i = j + 1;
    }
    out.extend_from_slice(&chars[se..]);
    *content = out.into_iter().collect();
}

fn byte_offset(content: &str, char_idx: usize) -> usize {
    content
        .char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(content.len())
}

/// Duplicate every line in `[s, e)` below the block. Returns the char index
/// where the duplicated block starts (the caret lands at its top).
pub fn duplicate_lines(content: &mut String, s: usize, e: usize) -> usize {
    let total = content.chars().count();
    let (ss, se) = lines_span(content, s, e);
    let block: String = content.chars().skip(ss).take(se - ss).collect();
    // The block carries its trailing newline when lines follow it; the last
    // line needs one added so the copy is a real line.
    let addition = if se == total {
        format!("\n{block}")
    } else {
        block
    };
    content.insert_str(byte_offset(content, se), &addition);
    se
}

/// Delete every line in `[s, e)`. Returns the caret char index (the line
/// that now sits where the block was).
pub fn delete_lines(content: &mut String, s: usize, e: usize) -> usize {
    let (ss, se) = lines_span(content, s, e);
    let (bs, be) = (byte_offset(content, ss), byte_offset(content, se));
    content.drain(bs..be);
    ss.min(content.chars().count())
}

/// Move the `[s, e)` block one line up (`dir < 0`) or down (`dir > 0`).
/// Returns the block's new span, or `None` when there is nothing to swap.
pub fn move_lines(content: &mut String, s: usize, e: usize, dir: i32) -> Option<(usize, usize)> {
    let total = content.chars().count();
    let (ss, se) = lines_span(content, s, e);
    let (ns, ne) = if dir < 0 {
        if ss == 0 {
            return None;
        }
        lines_span(content, ss - 1, ss - 1)
    } else {
        if se >= total {
            return None;
        }
        lines_span(content, se, se)
    };
    if ns == ss && ne == se {
        return None;
    }
    let cur: String = content.chars().skip(ss).take(se - ss).collect();
    let nei: String = content.chars().skip(ns).take(ne - ns).collect();
    let (bs, be) = (byte_offset(content, ss), byte_offset(content, se));
    let (bn, bne) = (byte_offset(content, ns), byte_offset(content, ne));
    let mut out = String::with_capacity(content.len());
    out.push_str(&content[..bn.min(bs)]);
    if dir < 0 {
        out.push_str(&cur);
        out.push_str(&content[bne..bs]);
        out.push_str(&nei);
    } else {
        out.push_str(&nei);
        out.push_str(&content[be..bn]);
        out.push_str(&cur);
    }
    out.push_str(&content[be.max(bne)..]);
    *content = out;
    let len = se - ss;
    Some((ns, ns + len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_ops_toggle_duplicate_move_delete() {
        let mut c = "fn main() {\n    let x = 1;\n    let y = 2;\n}\n".to_string();

        // Char 12 is the start of the `let x = 1;` line (lines start at 0,12,27,42).
        toggle_line_comment(&mut c, 12, 12);
        assert!(c.contains("    // let x = 1;"), "got {:?}", c);
        toggle_line_comment(&mut c, 12, 12);
        assert!(!c.contains("//"));

        // Duplicate the `let x` line and land on the copy (block 27..42).
        let caret = duplicate_lines(&mut c, 12, 12);
        assert_eq!(c, "fn main() {\n    let x = 1;\n    let x = 1;\n    let y = 2;\n}\n");
        assert_eq!(lines_span(&c, caret, caret), (27, 42));

        // Move the copied `let x` line down and back up.
        let (ns, ne) = move_lines(&mut c, caret, caret + 1, 1).unwrap();
        assert!(c[ns..ne].starts_with("    let x"));
        assert!(c.contains("    let y = 2;\n    let x = 1;\n"));
        move_lines(&mut c, ns, ne, -1).unwrap();
        assert_eq!(c, "fn main() {\n    let x = 1;\n    let x = 1;\n    let y = 2;\n}\n");

        // Delete a duplicated line, then the whole remaining block.
        delete_lines(&mut c, 12, 12);
        assert_eq!(c, "fn main() {\n    let x = 1;\n    let y = 2;\n}\n");
        let mut c2 = "fn main() {\n    let x = 1;\n    let y = 2;\n}\n".to_string();
        delete_lines(&mut c2, 12, 42);
        assert_eq!(c2, "fn main() {\n}\n");
        assert_eq!(lines_span(&c2, 0, 0), (0, 12));
    }
}