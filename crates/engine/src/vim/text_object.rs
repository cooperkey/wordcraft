//! Text objects implementation for Vim mode in WordCraft.
//!
//! Supports `iw`, `aw`, `i"`, `a"`, `i'`, `a'`, `i(`, `a(`, `i[`, `a[`, `i{`, `a{`, `ip`, `ap`.

use crate::Session;
use wordcraft_doc::Pos;

/// Locate the range of a text object around the current caret position.
/// Returns (start, end) positions if a matching text object is found.
pub fn find_text_object(session: &Session, target: char, inner: bool) -> Option<(Pos, Pos)> {
    let focus = &session.sel.focus;
    let para = session.doc.para_at(focus)?;
    let text = &para.text;
    let off = focus.off.min(para.len());

    match target {
        'w' => find_word(focus, text, off, inner),
        '"' | '\'' | '`' => find_quote(focus, text, off, target, inner),
        'q' => find_quote(focus, text, off, '"', inner)
            .or_else(|| find_quote(focus, text, off, '\'', inner))
            .or_else(|| find_quote(focus, text, off, '`', inner)),
        '(' | ')' | 'b' => find_pair(focus, text, off, '(', ')', inner),
        '[' | ']' => find_pair(focus, text, off, '[', ']', inner),
        '{' | '}' | 'B' => find_pair(focus, text, off, '{', '}', inner),
        '<' | '>' => find_pair(focus, text, off, '<', '>', inner),
        'p' => find_paragraph(session, focus, inner),
        's' => {
            let (a, b) = para.sentence_at(off);
            Some((Pos { off: a, ..focus.clone() }, Pos { off: b, ..focus.clone() }))
        }
        _ => None,
    }
}

fn find_word(focus: &Pos, text: &str, off: usize, inner: bool) -> Option<(Pos, Pos)> {
    let is_w = |c: char| wordcraft_doc::bidi::is_word_char(c);

    let mut start = off;
    for (i, c) in text.get(..off).unwrap_or("").char_indices().rev() {
        if !is_w(c) {
            break;
        }
        start = i;
    }

    let mut end = off;
    for (i, c) in text.get(off..).unwrap_or("").char_indices() {
        if !is_w(c) {
            if i == 0 && start == off {
                end = off + c.len_utf8();
            }
            break;
        }
        end = off + i + c.len_utf8();
    }

    if !inner {
        // 'aw' includes trailing whitespace if present, or preceding whitespace if at end
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

fn find_quote(focus: &Pos, text: &str, off: usize, quote: char, inner: bool) -> Option<(Pos, Pos)> {
    let mut indices = Vec::new();
    for (i, c) in text.char_indices() {
        let is_match = if quote == '"' {
            c == '"' || c == '“' || c == '”'
        } else if quote == '\'' {
            c == '\'' || c == '‘' || c == '’'
        } else {
            c == quote
        };
        if is_match {
            indices.push((i, c.len_utf8()));
        }
    }

    if indices.len() < 2 {
        return None;
    }

    // Find the pair enclosing or surrounding `off`
    for w in indices.windows(2) {
        let (open, open_len) = w[0];
        let (close, close_len) = w[1];
        if (open <= off && off <= close) || (open > off) {
            return if inner {
                let s = open + open_len;
                if s <= close {
                    Some((Pos { off: s, ..focus.clone() }, Pos { off: close, ..focus.clone() }))
                } else {
                    Some((Pos { off: close, ..focus.clone() }, Pos { off: close, ..focus.clone() }))
                }
            } else {
                Some((Pos { off: open, ..focus.clone() }, Pos { off: close + close_len, ..focus.clone() }))
            };
        }
    }

    None
}

fn find_pair(focus: &Pos, text: &str, off: usize, open: char, close: char, inner: bool) -> Option<(Pos, Pos)> {
    // Search backward for opening char
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

    // If caret is right on the open char itself
    if open_idx.is_none() && text.get(off..).unwrap_or("").starts_with(open) {
        open_idx = Some(off);
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
