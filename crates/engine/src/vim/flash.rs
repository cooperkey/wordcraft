//! Implementation of flash.nvim 2-character jump for WordCraft.

use wordcraft_doc::Pos;
use crate::{Selection, Session};

/// Perform a 2-character flash jump (`s{c1}{c2}`).
/// Finds the next occurrence of the 2-character pattern starting from the caret.
pub fn flash_jump(session: &mut Session, c1: char, c2: char) -> bool {
    let query = format!("{c1}{c2}");
    let focus = session.sel.focus.clone();
    let current_para_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
    let body_len = session.doc.body.len();

    // First search forward in the current paragraph past the cursor
    if let Some(para) = session.doc.para_at(&focus) {
        let current_off = focus.off.saturating_add(
            para.text.get(focus.off..)
                .and_then(|s| s.chars().next())
                .map(char::len_utf8)
                .unwrap_or(0)
        );
        if let Some(rest) = para.text.get(current_off..)
            && let Some(idx) = rest.find(&query)
        {
            let target_off = current_off.saturating_add(idx);
            session.sel = Selection::caret(Pos {
                off: target_off,
                ..focus
            });
            return true;
        }
    }

    // Search forward in subsequent paragraphs
    for p_idx in current_para_idx.saturating_add(1)..body_len {
        let pos = Pos::body(p_idx, 0);
        if let Some(para) = session.doc.para_at(&pos)
            && let Some(idx) = para.text.find(&query)
        {
            session.sel = Selection::caret(Pos::body(p_idx, idx));
            return true;
        }
    }

    // Wrap around to the start of the document if not found
    for p_idx in 0..=current_para_idx {
        let pos = Pos::body(p_idx, 0);
        if let Some(para) = session.doc.para_at(&pos)
            && let Some(idx) = para.text.find(&query)
        {
            session.sel = Selection::caret(Pos::body(p_idx, idx));
            return true;
        }
    }

    false
}
