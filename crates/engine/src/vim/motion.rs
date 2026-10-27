//! Inline character search motions (f, F, t, T, ;, ,) for Vim mode.

use serde::{Deserialize, Serialize};
use wordcraft_doc::Pos;
use crate::{Selection, Session};

/// An inline character search request (`f`, `F`, `t`, `T`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharFind {
    /// Target character to find on the current line.
    pub target: char,
    /// Whether searching forward (`f`, `t`) or backward (`F`, `T`).
    pub forward: bool,
    /// Whether "till" (`t`, `T`) stops before/after the target character.
    pub till: bool,
}

/// Find a character in `text` relative to `current_off`.
pub fn find_char_in_para(
    text: &str,
    current_off: usize,
    target: char,
    forward: bool,
    till: bool,
) -> Option<usize> {
    if forward {
        let slice = text.get(current_off..)?;
        let mut chars = slice.char_indices();
        // Skip current character
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
                    let next = text
                        .get(idx..)?
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| idx.saturating_add(i))
                        .unwrap_or(current_off);
                    return Some(next);
                }
                return Some(idx);
            }
        }
    }
    None
}

/// Execute an inline character search motion on the session.
/// Returns true if a match was found and caret/selection was updated.
pub fn execute_char_find(
    session: &mut Session,
    find: CharFind,
    extend: bool,
) -> bool {
    let focus = session.sel.focus.clone();
    let Some(para) = session.doc.para_at(&focus) else {
        return false;
    };

    let Some(target_off) = find_char_in_para(&para.text, focus.off, find.target, find.forward, find.till) else {
        return false;
    };

    let new_pos = Pos {
        off: target_off,
        ..focus
    };

    if extend {
        // When extending forward (e.g. `df.` or `vf.`), include the target character boundary
        let focus_pos = if find.forward && !find.till {
            let extra = para
                .text
                .get(target_off..)
                .and_then(|s| s.chars().next())
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            Pos {
                off: target_off.saturating_add(extra),
                ..new_pos.clone()
            }
        } else {
            new_pos
        };
        session.sel = Selection {
            anchor: session.sel.anchor.clone(),
            focus: focus_pos,
        };
    } else {
        session.sel = Selection::caret(new_pos);
    }

    true
}
