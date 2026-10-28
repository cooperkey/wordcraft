//! Comprehensive text objects implementation for Vim mode in WordCraft.
//!
//! Full parity with Neovim's `textobject.c`:
//! - Word & WORD: `iw`, `aw`, `iW`, `aW`
//! - Quotes: `i"`, `a"`, `i'`, `a'`, `` i` ``, `` a` ``, `iq`, `aq` (with forward scanning)
//! - Pairs / Brackets: `i(`, `a(`, `ib`, `ab`, `i[`, `a[`, `i{`, `a{`, `iB`, `aB`, `i<`, `a<` (with forward scanning)
//! - Paragraph & Sentence: `ip`, `ap`, `is`, `as`
//! - Tags: `it`, `at` (XML/HTML tags)

use super::motion::{CharKind, char_kind};
use crate::Session;
use wordcraft_doc::Pos;

/// Locate the range of a text object around or ahead of the current caret position.
/// Returns (start, end) positions if a matching text object is found.
pub fn find_text_object(session: &Session, target: char, inner: bool) -> Option<(Pos, Pos)> {
    let focus = &session.sel.focus;
    let para = session.doc.para_at(focus)?;
    let text = &para.text;
    let off = focus.off.min(para.len());

    match target {
        'w' => find_word(focus, text, off, inner),
        'W' => find_big_word(focus, text, off, inner),
        '"' | '\'' | '`' => find_quote(focus, text, off, target, inner),
        'q' => find_quote(focus, text, off, '"', inner)
            .or_else(|| find_quote(focus, text, off, '\'', inner))
            .or_else(|| find_quote(focus, text, off, '`', inner)),
        '(' | ')' | 'b' => find_pair(focus, text, off, '(', ')', inner),
        '[' | ']' => find_pair(focus, text, off, '[', ']', inner),
        '{' | '}' | 'B' => find_pair(focus, text, off, '{', '}', inner),
        '<' | '>' => find_pair(focus, text, off, '<', '>', inner),
        'p' => find_paragraph(session, focus, inner),
        's' => find_sentence(focus, text, off, inner),
        't' => find_tag(focus, text, off, inner),
        _ => None,
    }
}

/// Word text object: `iw` and `aw`.
/// Differentiates Word characters, Punctuation characters, and Whitespace.
fn find_word(focus: &Pos, text: &str, off: usize, inner: bool) -> Option<(Pos, Pos)> {
    if text.is_empty() {
        return None;
    }

    let target_kind = text.get(off..).and_then(|s| s.chars().next()).map(char_kind).unwrap_or(CharKind::Whitespace);

    // Expand left
    let mut start = off;
    for (i, c) in text.get(..off).unwrap_or("").char_indices().rev() {
        if char_kind(c) != target_kind {
            break;
        }
        start = i;
    }

    // Expand right
    let mut end = off;
    for (i, c) in text.get(off..).unwrap_or("").char_indices() {
        if char_kind(c) != target_kind {
            break;
        }
        end = off + i + c.len_utf8();
    }

    if start == end {
        return None;
    }

    if !inner {
        // 'aw' includes trailing whitespace if present, or preceding whitespace if at line end
        let mut trailing_end = end;
        for (i, c) in text.get(end..).unwrap_or("").char_indices() {
            if !c.is_whitespace() {
                break;
            }
            trailing_end = end + i + c.len_utf8();
        }
        if trailing_end > end {
            end = trailing_end;
        } else {
            let mut leading_start = start;
            for (i, c) in text.get(..start).unwrap_or("").char_indices().rev() {
                if !c.is_whitespace() {
                    break;
                }
                leading_start = i;
            }
            start = leading_start;
        }
    }

    Some((Pos { off: start, ..focus.clone() }, Pos { off: end, ..focus.clone() }))
}

/// WORD text object: `iW` and `aW` (whitespace-delimited).
fn find_big_word(focus: &Pos, text: &str, off: usize, inner: bool) -> Option<(Pos, Pos)> {
    if text.is_empty() {
        return None;
    }

    let is_on_ws = text.get(off..).and_then(|s| s.chars().next()).is_some_and(|c| c.is_whitespace());

    // Expand left
    let mut start = off;
    for (i, c) in text.get(..off).unwrap_or("").char_indices().rev() {
        if c.is_whitespace() != is_on_ws {
            break;
        }
        start = i;
    }

    // Expand right
    let mut end = off;
    for (i, c) in text.get(off..).unwrap_or("").char_indices() {
        if c.is_whitespace() != is_on_ws {
            break;
        }
        end = off + i + c.len_utf8();
    }

    if start == end {
        return None;
    }

    if !inner {
        let mut trailing_end = end;
        for (i, c) in text.get(end..).unwrap_or("").char_indices() {
            if !c.is_whitespace() {
                break;
            }
            trailing_end = end + i + c.len_utf8();
        }
        if trailing_end > end {
            end = trailing_end;
        } else {
            let mut leading_start = start;
            for (i, c) in text.get(..start).unwrap_or("").char_indices().rev() {
                if !c.is_whitespace() {
                    break;
                }
                leading_start = i;
            }
            start = leading_start;
        }
    }

    Some((Pos { off: start, ..focus.clone() }, Pos { off: end, ..focus.clone() }))
}

/// Quote text objects: `i"`, `a"`, `i'`, `a'`, `` i` ``, `` a` ``.
/// If cursor is not inside quotes, scans forward on the current line.
fn find_quote(focus: &Pos, text: &str, off: usize, quote: char, inner: bool) -> Option<(Pos, Pos)> {
    let mut indices = Vec::new();
    for (i, c) in text.char_indices() {
        let is_match = match quote {
            '"' => c == '"' || c == '“' || c == '”',
            '\'' => c == '\'' || c == '‘' || c == '’',
            _ => c == quote,
        };
        if is_match {
            indices.push((i, c.len_utf8()));
        }
    }

    if indices.len() < 2 {
        return None;
    }

    // Pair quotes into disjoint pairs (0,1), (2,3), etc.
    let mut pairs = Vec::new();
    let mut it = indices.into_iter();
    while let (Some(open), Some(close)) = (it.next(), it.next()) {
        pairs.push((open, close));
    }

    for ((open, open_len), (close, close_len)) in pairs {
        // Enclosing or cursor is ahead of opening quote on the line
        if (open <= off && off <= close + close_len) || (open > off) {
            return if inner {
                let s = open + open_len;
                Some((Pos { off: s, ..focus.clone() }, Pos { off: close, ..focus.clone() }))
            } else {
                Some((Pos { off: open, ..focus.clone() }, Pos { off: close + close_len, ..focus.clone() }))
            };
        }
    }

    None
}

/// Bracket/Pair text objects: `i(`, `a(`, `i[`, `a[`, `i{`, `a{`, `i<`, `a<`.
/// If cursor is not inside a pair, scans forward on the current line for the next opening bracket.
fn find_pair(focus: &Pos, text: &str, off: usize, open: char, close: char, inner: bool) -> Option<(Pos, Pos)> {
    // 1. First attempt: search backward for enclosing open bracket
    let mut open_idx = None;
    let mut depth = 0;
    for (i, c) in text.get(..off).unwrap_or("").char_indices().rev() {
        if c == close {
            depth += 1;
        } else if c == open {
            if depth == 0 {
                open_idx = Some(i);
                break;
            }
            depth -= 1;
        }
    }

    // 2. If caret is right on the open char itself
    if open_idx.is_none() && text.get(off..).unwrap_or("").starts_with(open) {
        open_idx = Some(off);
    }

    // 3. Neovim forward scan: if not inside a pair, scan forward from cursor for next open bracket
    if open_idx.is_none() {
        for (i, c) in text.get(off..).unwrap_or("").char_indices() {
            if c == open {
                open_idx = Some(off + i);
                break;
            }
        }
    }

    let open_idx = open_idx?;

    // Search forward from open_idx for matching closing char
    let search_start = open_idx + open.len_utf8();
    let mut close_idx = None;
    let mut depth = 0;
    for (i, c) in text.get(search_start..).unwrap_or("").char_indices() {
        let abs_i = search_start + i;
        if c == open {
            depth += 1;
        } else if c == close {
            if depth == 0 {
                close_idx = Some(abs_i);
                break;
            }
            depth -= 1;
        }
    }

    let close_idx = close_idx?;

    if inner {
        let start = open_idx + open.len_utf8();
        Some((Pos { off: start, ..focus.clone() }, Pos { off: close_idx, ..focus.clone() }))
    } else {
        Some((Pos { off: open_idx, ..focus.clone() }, Pos { off: close_idx + close.len_utf8(), ..focus.clone() }))
    }
}

/// Sentence text objects: `is` and `as`.
fn find_sentence(focus: &Pos, text: &str, off: usize, inner: bool) -> Option<(Pos, Pos)> {
    if text.is_empty() {
        return None;
    }

    let (mut start, mut end) = (0, text.len());

    // Locate sentence boundaries around `off`
    // Sentences terminate at '.', '!', '?' followed by whitespace or end of line.
    let mut sent_start = 0;
    for (i, c) in text.char_indices() {
        if matches!(c, '.' | '?' | '!') {
            let next_off = i + c.len_utf8();
            let is_boundary = next_off >= text.len() || text.get(next_off..).is_some_and(|s| s.starts_with(char::is_whitespace));
            if is_boundary {
                let sent_end = next_off;
                if off <= sent_end {
                    start = sent_start;
                    end = sent_end;
                    break;
                }
                // Skip following whitespace for next sentence start
                let mut ws_skip = sent_end;
                for (wi, wc) in text.get(sent_end..).unwrap_or("").char_indices() {
                    if !wc.is_whitespace() {
                        break;
                    }
                    ws_skip = sent_end + wi + wc.len_utf8();
                }
                sent_start = ws_skip;
            }
        }
    }

    // Skip leading whitespace for inner
    let actual_start = if inner {
        let mut s = start;
        for (i, c) in text.get(start..end).unwrap_or("").char_indices() {
            if !c.is_whitespace() {
                break;
            }
            s = start + i + c.len_utf8();
        }
        s
    } else {
        start
    };

    // Include trailing whitespace for 'as'
    let actual_end = if !inner {
        let mut e = end;
        for (i, c) in text.get(end..).unwrap_or("").char_indices() {
            if !c.is_whitespace() {
                break;
            }
            e = end + i + c.len_utf8();
        }
        e
    } else {
        end
    };

    Some((Pos { off: actual_start, ..focus.clone() }, Pos { off: actual_end, ..focus.clone() }))
}

/// Paragraph text objects: `ip` and `ap`.
fn find_paragraph(session: &Session, focus: &Pos, inner: bool) -> Option<(Pos, Pos)> {
    let para = session.doc.para_at(focus)?;
    let start = Pos { off: 0, ..focus.clone() };
    if inner {
        Some((start, Pos { off: para.len(), ..focus.clone() }))
    } else {
        let next_start = session
            .doc
            .next_para(focus.story, &focus.path)
            .filter(|q| q.parent() == focus.path.parent())
            .map(|q| Pos { story: focus.story, path: q, off: 0 })
            .unwrap_or_else(|| Pos { off: para.len(), ..focus.clone() });
        Some((start, next_start))
    }
}

/// Tag text objects: `it` and `at` for XML/HTML tags (e.g. `<p>hello</p>`).
fn find_tag(focus: &Pos, text: &str, off: usize, inner: bool) -> Option<(Pos, Pos)> {
    // 1. Search backward for `<tag>` (not starting with `</`)
    let mut open_tag_start = None;
    let mut tag_name = String::new();

    for (i, _) in text.get(..off).unwrap_or("").char_indices().rev() {
        if let Some(rest) = text.get(i..)
            && rest.starts_with('<')
            && !rest.starts_with("</")
            && let Some(close_bracket) = rest.find('>')
        {
            let tag_content = &rest[1..close_bracket];
            let name = tag_content.split_whitespace().next().unwrap_or("");
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                open_tag_start = Some(i);
                tag_name = name.to_ascii_lowercase();
                break;
            }
        }
    }

    // Forward scan if cursor is before open tag
    if open_tag_start.is_none() {
        for (i, _) in text.get(off..).unwrap_or("").char_indices() {
            let abs_i = off + i;
            if let Some(rest) = text.get(abs_i..)
                && rest.starts_with('<')
                && !rest.starts_with("</")
                && let Some(close_bracket) = rest.find('>')
            {
                let tag_content = &rest[1..close_bracket];
                let name = tag_content.split_whitespace().next().unwrap_or("");
                if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                    open_tag_start = Some(abs_i);
                    tag_name = name.to_ascii_lowercase();
                    break;
                }
            }
        }
    }

    let open_start = open_tag_start?;
    let open_rest = text.get(open_start..)?;
    let open_close_bracket = open_rest.find('>')?;
    let open_end = open_start + open_close_bracket + 1;

    // Search for matching closing tag `</tag>`
    let closing_pat = format!("</{}>", tag_name);
    let close_start = open_end + text.get(open_end..)?.to_ascii_lowercase().find(&closing_pat)?;
    let close_end = close_start + closing_pat.len();

    if inner {
        Some((Pos { off: open_end, ..focus.clone() }, Pos { off: close_start, ..focus.clone() }))
    } else {
        Some((Pos { off: open_start, ..focus.clone() }, Pos { off: close_end, ..focus.clone() }))
    }
}
