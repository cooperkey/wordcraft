//! Insert and Replace mode handlers for Vim mode in WordCraft.
//!
//! Full parity with Neovim's `insert.c`:
//! - Keychords:
//!   - `<C-w>`: Delete previous word before cursor.
//!   - `<C-u>`: Delete from cursor to start of paragraph.
//!   - `<C-h>`: Backspace alias.
//!   - `<C-j>` / `<C-m>`: Insert newline.
//!   - `<C-t>`: Indent line.
//!   - `<C-d>`: Dedent line.
//!   - `<C-r>{reg}`: Inline register paste.
//!   - `<C-o>`: One-shot normal mode command.
//!   - `<C-a>`: Re-insert last inserted text.
//! - Replace Mode (`R`):
//!   - Overwrites characters under the cursor.
//!   - `Backspace` restores the original overwritten character.
//!   - `Escape` returns to Normal mode.

use super::{VimMode, VimState};
use crate::{Selection, Session};
use serde_json::json;

/// Handle keystrokes while in Insert mode.
pub fn handle_insert_key(
    vim: &mut VimState,
    session: &mut Session,
    key_name: &str,
    text: Option<&str>,
    ctrl: bool,
    _alt: bool,
    _shift: bool,
) -> bool {
    let input_char = text.and_then(|t| t.chars().next());

    if ctrl && key_name == "Q" {
        vim.unicode_hex_layer = Some(String::new());
        vim.status_msg = Some(super::unicode::format_display(""));
        return true;
    }

    if let Some(mut hex_buf) = vim.unicode_hex_layer.clone() {
        let result = super::unicode::handle_hex_key(session, &mut hex_buf, key_name, input_char);
        match result {
            super::unicode::HexKeyResult::Continue => {
                vim.unicode_hex_layer = Some(hex_buf.clone());
                vim.status_msg = Some(super::unicode::format_display(&hex_buf));
                return true;
            }
            super::unicode::HexKeyResult::Inserted | super::unicode::HexKeyResult::Cancel => {
                vim.unicode_hex_layer = None;
                vim.status_msg = None;
                return true;
            }
        }
    }

    // Register paste pending: <C-r>{reg}
    if vim.pending == "<C-r>" {
        if let Some(r) = input_char
            && let Some(val) = vim.registers.get_value(r)
        {
            let _ = session.run("text.insert", &json!({"text": val.text}));
            vim.insert_buf.push_str(&val.text);
        }
        vim.pending.clear();
        return true;
    }

    // Ctrl shortcuts in Insert mode
    if ctrl {
        match key_name {
            "W" => {
                // <C-w> delete word before cursor
                let focus = session.sel.focus.clone();
                if let Some(para) = session.doc.para_at(&focus) {
                    let off = focus.off.min(para.len());
                    let prev_off = super::motion::word_backward(&para.text, off);
                    if prev_off < off {
                        let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                        let pa = wordcraft_doc::Pos::body(p_idx, prev_off);
                        let pb = wordcraft_doc::Pos::body(p_idx, off);
                        let _ = session.doc.delete_range(&pa, &pb);
                        session.sel = Selection::caret(pa);
                        session.relayout();
                    }
                }
                return true;
            }
            "U" => {
                // <C-u> delete from cursor to line start
                let focus = session.sel.focus.clone();
                if focus.off > 0 {
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    let pa = wordcraft_doc::Pos::body(p_idx, 0);
                    let pb = wordcraft_doc::Pos::body(p_idx, focus.off);
                    let _ = session.doc.delete_range(&pa, &pb);
                    session.sel = Selection::caret(pa);
                    session.relayout();
                }
                return true;
            }
            "H" => {
                // <C-h> backspace
                let _ = session.run("text.backspace", &json!({}));
                return true;
            }
            "J" | "M" => {
                // <C-j> / <C-m> newline
                let _ = session.run("text.newParagraph", &json!({}));
                return true;
            }
            "T" => {
                // <C-t> indent
                let _ = session.run("para.indent", &json!({}));
                return true;
            }
            "D" => {
                // <C-d> dedent
                let _ = session.run("para.outdent", &json!({}));
                return true;
            }
            "R" => {
                // <C-r> register paste
                vim.pending = "<C-r>".to_string();
                return true;
            }
            "O" => {
                // <C-o> one-shot normal mode
                vim.one_shot_normal = true;
                vim.mode = VimMode::Normal;
                vim.status_msg = Some("-- (insert) --".to_string());
                return true;
            }
            "A" => {
                // <C-a> re-insert last inserted text
                if !vim.registers.last_inserted.is_empty() {
                    let ins = vim.registers.last_inserted.clone();
                    let _ = session.run("text.insert", &json!({"text": ins}));
                    vim.insert_buf.push_str(&ins);
                }
                return true;
            }
            _ => {}
        }
    }

    if key_name == "Escape" || (ctrl && key_name == "[") {
        if let Some(block) = vim.block_edit.take()
            && !vim.insert_buf.is_empty()
        {
            for line_idx in block.lines {
                let pos = wordcraft_doc::Pos::body(line_idx, 0);
                if let Some(p) = session.doc.para_at(&pos) {
                    let target_off = block.col.min(p.len());
                    session.sel = Selection::caret(wordcraft_doc::Pos::body(line_idx, target_off));
                    let _ = session.run("text.insert", &json!({"text": vim.insert_buf}));
                }
            }
            session.relayout();
        }

        match &mut vim.last_change {
            Some(super::LastVimChange::ChangeToEnd { text }) if text.is_empty() => {
                *text = vim.insert_buf.clone();
            }
            Some(super::LastVimChange::SubstituteLine { text }) if text.is_empty() => {
                *text = vim.insert_buf.clone();
            }
            _ => {}
        }

        vim.marks.set('^', session.sel.focus.clone());
        if !vim.insert_buf.is_empty() {
            vim.registers.last_inserted = std::mem::take(&mut vim.insert_buf);
        }
        let _ = session.run("caret.left", &json!({}));
        vim.mode = VimMode::Normal;
        vim.reset_pending();
        return true;
    }

    // Normal typing is handled by egui / text insertion events
    false
}

/// Handle keystrokes while in Replace mode (`R`).
pub fn handle_replace_key(vim: &mut VimState, session: &mut Session, key_name: &str, text: Option<&str>) -> bool {
    if key_name == "Escape" {
        vim.marks.set('^', session.sel.focus.clone());
        vim.replace_stack.clear();
        let _ = session.run("caret.left", &json!({}));
        vim.mode = VimMode::Normal;
        vim.status_msg = None;
        vim.reset_pending();
        return true;
    }

    if key_name == "Backspace" {
        if let Some((pos, orig_char)) = vim.replace_stack.pop() {
            session.sel = Selection::caret(pos.clone());
            let _ = session.run("text.delete", &json!({}));
            let _ = session.run("text.insert", &json!({"text": orig_char}));
            session.sel = Selection::caret(pos);
        }
        return true;
    }

    if let Some(ch) = text.and_then(|t| t.chars().next())
        && !ch.is_control()
    {
        let focus = session.sel.focus.clone();
        if let Some(para) = session.doc.para_at(&focus) {
            if focus.off < para.len() {
                let old_char = para.text.get(focus.off..).and_then(|s| s.chars().next()).unwrap_or(' ');
                vim.replace_stack.push((focus.clone(), old_char.to_string()));
                let _ = session.run("text.delete", &json!({}));
            }
            let _ = session.run("text.insert", &json!({"text": ch.to_string()}));
        }
        return true;
    }

    false
}
