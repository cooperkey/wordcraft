//! Native Vim motion engine for WordCraft.
//!
//! Provides word, WORD, sentence, paragraph, bracket, and viewport motions
//! matching Neovim's `normal.c` and `charset.c` specifications.

use crate::{Selection, Session};
use serde::{Deserialize, Serialize};
use wordcraft_doc::Pos;

/// Inline character search request (`f`, `F`, `t`, `T`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharFind {
    /// Target character to find on the current line.
    pub target: char,
    /// Whether searching forward (`f`, `t`) or backward (`F`, `T`).
    pub forward: bool,
    /// Whether "till" (`t`, `T`) stops before/after the target character.
    pub till: bool,
}

/// Character classification matching Neovim's `charset.c`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CharKind {
    /// Whitespace (` ` or `\t`).
    Whitespace,
    /// Word character (alphanumeric or `_`).
    Word,
    /// Punctuation or symbol.
    Punctuation,
}

/// Classify a character into whitespace, word, or punctuation.
pub fn char_kind(c: char) -> CharKind {
    if c.is_whitespace() {
        CharKind::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        CharKind::Word
    } else {
        CharKind::Punctuation
    }
}

/// Move forward to the start of the next word (`w`).
pub fn word_forward(text: &str, off: usize) -> usize {
    let Some(slice) = text.get(off..) else {
        return text.len();
    };
    let mut chars = slice.char_indices();
    let Some((_, first_char)) = chars.next() else {
        return text.len();
    };
    let first_kind = char_kind(first_char);

    let mut next_off = off + first_char.len_utf8();
    if first_kind != CharKind::Whitespace {
        for (i, c) in chars.by_ref() {
            if char_kind(c) != first_kind {
                next_off = off + i;
                break;
            }
            next_off = off + i + c.len_utf8();
        }
    }

    if let Some(rest) = text.get(next_off..) {
        for (i, c) in rest.char_indices() {
            if !c.is_whitespace() {
                return next_off + i;
            }
        }
    }

    text.len()
}

/// Move forward to the end of the current or next word (`e`).
pub fn word_end(text: &str, off: usize) -> usize {
    if off >= text.len() {
        return text.len();
    }
    let Some(first_char) = text.get(off..).and_then(|s| s.chars().next()) else {
        return text.len();
    };
    let start_off = off + first_char.len_utf8();
    let Some(rest) = text.get(start_off..) else {
        return off;
    };

    let mut word_start = None;
    let mut word_k = None;
    for (i, c) in rest.char_indices() {
        let k = char_kind(c);
        if k != CharKind::Whitespace {
            word_start = Some(start_off + i);
            word_k = Some(k);
            break;
        }
    }

    let Some(start) = word_start else {
        return off;
    };
    let Some(k) = word_k else {
        return off;
    };
    let mut end_char_off = start;

    if let Some(word_rest) = text.get(start..) {
        for (i, c) in word_rest.char_indices() {
            if char_kind(c) == k {
                end_char_off = start + i;
            } else {
                break;
            }
        }
    }

    end_char_off
}

/// Move backward to the start of the previous or current word (`b`).
pub fn word_backward(text: &str, off: usize) -> usize {
    if off == 0 {
        return 0;
    }
    let Some(before) = text.get(..off) else {
        return 0;
    };
    let mut rev = before.char_indices().rev();

    let mut target_idx = None;
    let mut target_kind = None;
    for (i, c) in rev.by_ref() {
        let k = char_kind(c);
        if k != CharKind::Whitespace {
            target_idx = Some(i);
            target_kind = Some(k);
            break;
        }
    }

    let Some(mut start_idx) = target_idx else {
        return 0;
    };
    let Some(kind) = target_kind else {
        return 0;
    };

    for (i, c) in rev {
        if char_kind(c) == kind {
            start_idx = i;
        } else {
            break;
        }
    }

    start_idx
}

/// Move backward to the end of the previous word (`ge`).
pub fn word_end_backward(text: &str, off: usize) -> usize {
    if off == 0 {
        return 0;
    }
    let Some(before) = text.get(..off) else {
        return 0;
    };
    let mut rev = before.char_indices().rev();
    let _ = rev.next();

    for (i, c) in rev {
        if !c.is_whitespace() {
            return i;
        }
    }
    0
}

/// Move forward to the start of the next WORD (`W`).
pub fn big_word_forward(text: &str, off: usize) -> usize {
    let Some(slice) = text.get(off..) else {
        return text.len();
    };
    let mut chars = slice.char_indices();
    let Some((_, first_char)) = chars.next() else {
        return text.len();
    };

    let mut next_off = off + first_char.len_utf8();
    if !first_char.is_whitespace() {
        for (i, c) in chars.by_ref() {
            if c.is_whitespace() {
                next_off = off + i;
                break;
            }
            next_off = off + i + c.len_utf8();
        }
    }

    if let Some(rest) = text.get(next_off..) {
        for (i, c) in rest.char_indices() {
            if !c.is_whitespace() {
                return next_off + i;
            }
        }
    }

    text.len()
}

/// Move forward to the end of the current or next WORD (`E`).
pub fn big_word_end(text: &str, off: usize) -> usize {
    if off >= text.len() {
        return text.len();
    }
    let Some(first_char) = text.get(off..).and_then(|s| s.chars().next()) else {
        return text.len();
    };
    let start_off = off + first_char.len_utf8();
    let Some(rest) = text.get(start_off..) else {
        return off;
    };

    let mut word_start = None;
    for (i, c) in rest.char_indices() {
        if !c.is_whitespace() {
            word_start = Some(start_off + i);
            break;
        }
    }

    let Some(start) = word_start else {
        return off;
    };
    let mut end_char_off = start;

    if let Some(word_rest) = text.get(start..) {
        for (i, c) in word_rest.char_indices() {
            if !c.is_whitespace() {
                end_char_off = start + i;
            } else {
                break;
            }
        }
    }

    end_char_off
}

/// Move backward to the start of the previous or current WORD (`B`).
pub fn big_word_backward(text: &str, off: usize) -> usize {
    if off == 0 {
        return 0;
    }
    let Some(before) = text.get(..off) else {
        return 0;
    };
    let mut rev = before.char_indices().rev();

    let mut target_idx = None;
    for (i, c) in rev.by_ref() {
        if !c.is_whitespace() {
            target_idx = Some(i);
            break;
        }
    }

    let Some(mut start_idx) = target_idx else {
        return 0;
    };
    for (i, c) in rev {
        if !c.is_whitespace() {
            start_idx = i;
        } else {
            break;
        }
    }

    start_idx
}

/// Move backward to the end of the previous WORD (`gE`).
pub fn big_word_end_backward(text: &str, off: usize) -> usize {
    if off == 0 {
        return 0;
    }
    let Some(before) = text.get(..off) else {
        return 0;
    };
    let mut rev = before.char_indices().rev();
    let _ = rev.next();

    for (i, c) in rev {
        if !c.is_whitespace() {
            return i;
        }
    }
    0
}

/// First non-whitespace character in paragraph (`^`).
pub fn first_non_blank(text: &str) -> usize {
    text.char_indices().find(|(_, c)| !c.is_whitespace()).map(|(i, _)| i).unwrap_or(0)
}

/// Last non-whitespace character in paragraph (`g_`).
pub fn last_non_blank(text: &str) -> usize {
    text.char_indices().rfind(|(_, c)| !c.is_whitespace()).map(|(i, _)| i).unwrap_or_else(|| text.len().saturating_sub(1))
}

/// 1-based column position in paragraph (`|`).
pub fn col_offset(text: &str, col: usize) -> usize {
    let target = col.saturating_sub(1);
    let mut last_off = 0;
    for (char_count, (off, _)) in text.char_indices().enumerate() {
        if char_count == target {
            return off;
        }
        last_off = off;
    }
    last_off
}

/// Move forward to the next sentence start (`)`).
pub fn sentence_forward(session: &Session, pos: &Pos) -> Pos {
    let p_idx = pos.path.0.first().copied().unwrap_or(0) as usize;
    if let Some(para) = session.doc.para_at(pos)
        && let Some(rest) = para.text.get(pos.off..)
    {
        for (i, c) in rest.char_indices() {
            if matches!(c, '.' | '?' | '!') {
                let after = pos.off + i + c.len_utf8();
                if let Some(tail) = para.text.get(after..) {
                    for (j, tc) in tail.char_indices() {
                        if !tc.is_whitespace() {
                            return Pos::body(p_idx, after + j);
                        }
                    }
                }
            }
        }
    }
    if p_idx + 1 < session.doc.body.len() {
        let next_p = p_idx + 1;
        let off = session.doc.para_at(&Pos::body(next_p, 0)).map(|p| first_non_blank(&p.text)).unwrap_or(0);
        return Pos::body(next_p, off);
    }
    pos.clone()
}

/// Move backward to the previous sentence start (`(`).
pub fn sentence_backward(session: &Session, pos: &Pos) -> Pos {
    let p_idx = pos.path.0.first().copied().unwrap_or(0) as usize;
    if let Some(para) = session.doc.para_at(pos)
        && let Some(before) = para.text.get(..pos.off)
    {
        let mut boundaries: Vec<usize> = Vec::new();
        let mut prev_punct = None;
        for (i, c) in before.char_indices() {
            if let Some(punct_end) = prev_punct
                && !c.is_whitespace()
                && i > punct_end
            {
                boundaries.push(i);
                prev_punct = None;
            }
            if matches!(c, '.' | '?' | '!') {
                prev_punct = Some(i + c.len_utf8());
            }
        }
        if let Some(&last_boundary) = boundaries.last()
            && last_boundary < pos.off
        {
            return Pos::body(p_idx, last_boundary);
        }
        let fnb = first_non_blank(&para.text);
        if pos.off > fnb {
            return Pos::body(p_idx, fnb);
        }
    }
    if p_idx > 0 {
        let prev_p = p_idx - 1;
        let off = session.doc.para_at(&Pos::body(prev_p, 0)).map(|p| first_non_blank(&p.text)).unwrap_or(0);
        return Pos::body(prev_p, off);
    }
    Pos::body(0, 0)
}

/// Find matching bracket `()`, `[]`, `{}`, `<>` (`%`).
pub fn find_matching_bracket(text: &str, off: usize) -> Option<usize> {
    let ch = text.get(off..)?.chars().next()?;
    let (open, close, forward) = match ch {
        '(' => ('(', ')', true),
        ')' => ('(', ')', false),
        '[' => ('[', ']', true),
        ']' => ('[', ']', false),
        '{' => ('{', '}', true),
        '}' => ('{', '}', false),
        '<' => ('<', '>', true),
        '>' => ('<', '>', false),
        _ => {
            let rest = text.get(off..)?;
            for (i, c) in rest.char_indices() {
                if matches!(c, '(' | '[' | '{' | '<') {
                    return find_matching_bracket(text, off + i);
                }
            }
            return None;
        }
    };
    let mut depth: i32 = 0;
    if forward {
        for (i, c) in text.get(off..)?.char_indices() {
            if c == open {
                depth += 1;
            }
            if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(off.saturating_add(i));
                }
            }
        }
    } else {
        for (i, c) in text.get(..off.saturating_add(ch.len_utf8()))?.char_indices().rev() {
            if c == close {
                depth += 1;
            }
            if c == open {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// Find previous or next section/heading paragraph (`[[` / `]]`).
pub fn section_jump(session: &Session, pos: &Pos, forward: bool) -> Pos {
    let p_idx = pos.path.0.first().copied().unwrap_or(0) as usize;
    let total = session.doc.body.len();
    let is_section = |para: &wordcraft_doc::Paragraph| {
        para.props.style.as_deref().is_some_and(|s| s.starts_with("Heading"))
            || para.text.starts_with("# ")
            || para.text.starts_with("## ")
            || para.text.starts_with("### ")
    };
    if forward {
        for i in (p_idx + 1)..total {
            if let Some(para) = session.doc.para_at(&Pos::body(i, 0))
                && is_section(para)
            {
                return Pos::body(i, 0);
            }
        }
        Pos::body(total.saturating_sub(1), 0)
    } else {
        for i in (0..p_idx).rev() {
            if let Some(para) = session.doc.para_at(&Pos::body(i, 0))
                && is_section(para)
            {
                return Pos::body(i, 0);
            }
        }
        Pos::body(0, 0)
    }
}

/// Find a character in `text` relative to `current_off`.
pub fn find_char_in_para(text: &str, current_off: usize, target: char, forward: bool, till: bool) -> Option<usize> {
    if forward {
        let slice = text.get(current_off..)?;
        let mut chars = slice.char_indices();
        let _ = chars.next();
        for (idx, c) in chars {
            if c == target {
                let match_off = current_off.saturating_add(idx);
                if till {
                    let before = text.get(..match_off)?;
                    let prev = before.char_indices().last().map(|(i, _)| i).unwrap_or(current_off);
                    return Some(prev);
                }
                return Some(match_off);
            }
        }
    } else {
        let before = text.get(..current_off)?;
        let rev = before.char_indices().rev();
        for (idx, c) in rev {
            if c == target {
                if till {
                    let next = text.get(idx..)?.char_indices().nth(1).map(|(i, _)| idx.saturating_add(i)).unwrap_or(current_off);
                    return Some(next);
                }
                return Some(idx);
            }
        }
    }
    None
}

/// Execute an inline character search motion on the session.
pub fn execute_char_find(session: &mut Session, find: CharFind, extend: bool) -> bool {
    let focus = session.sel.focus.clone();
    let Some(para) = session.doc.para_at(&focus) else {
        return false;
    };

    let Some(target_off) = find_char_in_para(&para.text, focus.off, find.target, find.forward, find.till) else {
        return false;
    };

    let new_pos = Pos { off: target_off, ..focus };

    if extend {
        let focus_pos = if find.forward && !find.till {
            let extra = para.text.get(target_off..).and_then(|s| s.chars().next()).map(|c| c.len_utf8()).unwrap_or(0);
            Pos { off: target_off.saturating_add(extra), ..new_pos.clone() }
        } else {
            new_pos
        };
        session.sel = Selection { anchor: session.sel.anchor.clone(), focus: focus_pos };
    } else {
        session.sel = Selection::caret(new_pos);
    }

    true
}
