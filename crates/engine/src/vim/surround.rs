//! Implementation of nvim-surround (ys, cs, ds) for WordCraft.

use super::text_object::find_text_object;
use crate::{Selection, Session};
use serde_json::json;
use wordcraft_doc::Pos;

/// Delimiter pair for a surround character: (open, close, add_space).
pub fn delimiter_pair(c: char) -> Option<(char, char, bool)> {
    match c {
        '(' => Some(('(', ')', true)),
        ')' => Some(('(', ')', false)),
        '[' => Some(('[', ']', true)),
        ']' => Some(('[', ']', false)),
        '{' => Some(('{', '}', true)),
        '}' => Some(('{', '}', false)),
        '<' => Some(('<', '>', true)),
        '>' => Some(('<', '>', false)),
        '"' => Some(('"', '"', false)),
        '\'' => Some(('\'', '\'', false)),
        '`' => Some(('`', '`', false)),
        '*' => Some(('*', '*', false)),
        '_' => Some(('_', '_', false)),
        '~' => Some(('~', '~', false)),
        _ => None,
    }
}

/// Delete surrounding delimiters (`ds{char}`).
pub fn delete_surround(session: &mut Session, target: char) -> bool {
    let Some((start, end)) = find_text_object(session, target, false) else {
        return false;
    };

    let focus = session.sel.focus.clone();
    let Some(para) = session.doc.para_at(&focus) else {
        return false;
    };

    // Calculate length of open delimiter and close delimiter
    let open_len = para.text.get(start.off..).and_then(|s| s.chars().next()).map(char::len_utf8).unwrap_or(1);

    // Close delimiter starts before end.off
    let close_start = end.off.saturating_sub(para.text.get(..end.off).and_then(|s| s.chars().last()).map(char::len_utf8).unwrap_or(1));

    // Delete closing delimiter first (to avoid invalidating start offset)
    session.sel = Selection { anchor: Pos { off: close_start, ..end.clone() }, focus: end.clone() };
    let _ = session.run("edit.cut", &json!({}));

    // Delete opening delimiter
    let open_end = Pos { off: start.off.saturating_add(open_len), ..start.clone() };
    session.sel = Selection { anchor: start.clone(), focus: open_end };
    let _ = session.run("edit.cut", &json!({}));

    session.sel = Selection::caret(start);
    true
}

/// Change surrounding delimiters (`cs{old}{new}`).
pub fn change_surround(session: &mut Session, old_char: char, new_char: char) -> bool {
    let Some((new_open, new_close, _)) = delimiter_pair(new_char) else {
        return false;
    };

    let Some((start, end)) = find_text_object(session, old_char, false) else {
        return false;
    };

    let focus = session.sel.focus.clone();
    let Some(para) = session.doc.para_at(&focus) else {
        return false;
    };

    let open_len = para.text.get(start.off..).and_then(|s| s.chars().next()).map(char::len_utf8).unwrap_or(1);

    let close_start = end.off.saturating_sub(para.text.get(..end.off).and_then(|s| s.chars().last()).map(char::len_utf8).unwrap_or(1));

    // Replace closing delimiter
    session.sel = Selection { anchor: Pos { off: close_start, ..end.clone() }, focus: end.clone() };
    let _ = session.run("text.insert", &json!({"text": new_close.to_string()}));

    // Replace opening delimiter
    let open_end = Pos { off: start.off.saturating_add(open_len), ..start.clone() };
    session.sel = Selection { anchor: start.clone(), focus: open_end };
    let _ = session.run("text.insert", &json!({"text": new_open.to_string()}));

    session.sel = Selection::caret(start);
    true
}

/// Wrap a selection with surrounding delimiters (`ys{motion/obj}{char}`).
pub fn wrap_selection(session: &mut Session, target_char: char) -> bool {
    let Some((open, close, space)) = delimiter_pair(target_char) else {
        return false;
    };

    let sel = session.sel.clone();
    let (start, end) = if sel.anchor.off <= sel.focus.off { (sel.anchor, sel.focus) } else { (sel.focus, sel.anchor) };

    let (open_str, close_str) = if space { (format!("{open} "), format!(" {close}")) } else { (open.to_string(), close.to_string()) };

    // Insert close delimiter at end
    session.sel = Selection::caret(end.clone());
    let _ = session.run("text.insert", &json!({"text": close_str}));

    // Insert open delimiter at start
    session.sel = Selection::caret(start.clone());
    let _ = session.run("text.insert", &json!({"text": open_str}));

    session.sel = Selection::caret(start);
    true
}
