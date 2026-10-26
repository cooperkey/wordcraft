//! Headless unit tests for Vim modal editing in WordCraft.

use serde_json::json;
use wordcraft_doc::{Document, Path, Pos};

use crate::{Selection, Session, VimMode};

fn new_session(text: &str) -> Session {
    let mut s = Session::new(Document::new());
    s.vim.enabled = true;
    s.vim.mode = VimMode::Normal;
    if !text.is_empty() {
        let lines: Vec<&str> = text.split('\n').collect();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                let _ = s.run("text.newParagraph", &json!({}));
            }
            if !line.is_empty() {
                let _ = s.run("text.insert", &json!({"text": line}));
            }
        }
    }
    s
}

#[test]
fn vim_mode_defaults_and_toggle() {
    let mut s = Session::new(Document::new());
    assert!(!s.vim.enabled);

    let res = s.run("vim.toggle", &json!({})).unwrap();
    assert_eq!(res.get("enabled").unwrap(), true);
    assert_eq!(s.vim.enabled, true);
    assert_eq!(s.vim.mode, VimMode::Normal);

    let res = s.run("vim.toggle", &json!({})).unwrap();
    assert_eq!(res.get("enabled").unwrap(), false);
    assert_eq!(s.vim.enabled, false);
}

#[test]
fn vim_motions_hjkl() {
    let mut s = new_session("Hello World\nLine two here");
    s.sel = Selection::caret(Pos::body(0, 5));

    // 'h' moves left
    assert!(s.handle_vim_key("H", Some("h"), false, false, false));
    assert_eq!(s.sel.focus.off, 4);

    // 'l' moves right
    assert!(s.handle_vim_key("L", Some("l"), false, false, false));
    assert_eq!(s.sel.focus.off, 5);

    // 'j' moves down to line 1
    assert!(s.handle_vim_key("J", Some("j"), false, false, false));
    assert_eq!(s.sel.focus.path, Path::top(1));

    // 'k' moves up to line 0
    assert!(s.handle_vim_key("K", Some("k"), false, false, false));
    assert_eq!(s.sel.focus.path, Path::top(0));
}

#[test]
fn vim_count_multipliers() {
    let mut s = new_session("abcdefghijklmnop");
    s.sel = Selection::caret(Pos::body(0, 0));

    // Type '3' then 'l' -> moves 3 characters right
    assert!(s.handle_vim_key("3", Some("3"), false, false, false));
    assert_eq!(s.vim.count, Some(3));
    assert!(s.handle_vim_key("L", Some("l"), false, false, false));
    assert_eq!(s.sel.focus.off, 3);
    assert_eq!(s.vim.count, None);
}

#[test]
fn vim_line_boundary_motions() {
    let mut s = new_session("The quick brown fox");
    s.sel = Selection::caret(Pos::body(0, 5));

    // '$' moves to end of line
    assert!(s.handle_vim_key("$", Some("$"), false, false, false));
    assert_eq!(s.sel.focus.off, 19);

    // '0' moves to start of line
    assert!(s.handle_vim_key("0", Some("0"), false, false, false));
    assert_eq!(s.sel.focus.off, 0);
}

#[test]
fn vim_insert_transitions_and_escape() {
    let mut s = new_session("Word");
    s.sel = Selection::caret(Pos::body(0, 2));

    // 'i' enters Insert mode without moving caret
    assert!(s.handle_vim_key("I", Some("i"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::Insert);
    assert_eq!(s.sel.focus.off, 2);

    // In Insert mode, Escape returns to Normal mode and steps left 1 char
    assert!(s.handle_vim_key("Escape", None, false, false, false));
    assert_eq!(s.vim.mode, VimMode::Normal);
    assert_eq!(s.sel.focus.off, 1);

    // 'a' steps right 1 char and enters Insert mode
    assert!(s.handle_vim_key("A", Some("a"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::Insert);
    assert_eq!(s.sel.focus.off, 2);
}

#[test]
fn vim_single_char_deletion_x() {
    let mut s = new_session("Rust");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'x' deletes the character under caret ('R')
    assert!(s.handle_vim_key("X", Some("x"), false, false, false));
    let text = s.doc.para_at(&s.sel.focus).unwrap().text.clone();
    assert_eq!(text, "ust");
}

#[test]
fn vim_undo_with_u() {
    let mut s = new_session("Hello");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'x' deletes 'H' -> "ello"
    assert!(s.handle_vim_key("X", Some("x"), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "ello");

    // 'u' undoes the deletion -> "Hello"
    assert!(s.handle_vim_key("U", Some("u"), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "Hello");
}

#[test]
fn vim_operator_pending_dw_and_cw() {
    let mut s = new_session("One two three");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'd' then 'w' -> deletes "One "
    assert!(s.handle_vim_key("D", Some("d"), false, false, false));
    assert_eq!(s.vim.pending_op, Some(crate::vim::VimOp::Delete));
    assert!(s.handle_vim_key("W", Some("w"), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "two three");
    assert_eq!(s.vim.mode, VimMode::Normal);

    // 'c' then 'w' -> deletes "two " and switches to Insert mode
    assert!(s.handle_vim_key("C", Some("c"), false, false, false));
    assert_eq!(s.vim.pending_op, Some(crate::vim::VimOp::Change));
    assert!(s.handle_vim_key("W", Some("w"), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "three");
    assert_eq!(s.vim.mode, VimMode::Insert);
}

#[test]
fn vim_text_object_inner_and_around_word() {
    let mut s = new_session("quick brown fox");
    s.sel = Selection::caret(Pos::body(0, 7)); // in "brown"

    // 'd' + 'i' + 'w' -> deletes "brown", leaving "quick  fox"
    assert!(s.handle_vim_key("D", Some("d"), false, false, false));
    assert!(s.handle_vim_key("I", Some("i"), false, false, false));
    assert_eq!(s.vim.pending, "i");
    assert!(s.handle_vim_key("W", Some("w"), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "quick  fox");
}

#[test]
fn vim_text_object_quotes() {
    let mut s = new_session(r#"echo "hello world" end"#);
    s.sel = Selection::caret(Pos::body(0, 8)); // in "hello"

    // 'c' + 'i' + '"' -> deletes "hello world", leaves `echo "" end`, switches to Insert
    assert!(s.handle_vim_key("C", Some("c"), false, false, false));
    assert!(s.handle_vim_key("I", Some("i"), false, false, false));
    assert!(s.handle_vim_key("Quote", Some("\""), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, r#"echo "" end"#);
    assert_eq!(s.vim.mode, VimMode::Insert);

    // Return to Normal mode
    assert!(s.handle_vim_key("Escape", None, false, false, false));
    assert_eq!(s.vim.mode, VimMode::Normal);

    // 'd' + 'a' + '"' -> deletes `"" ` (around quotes)
    assert!(s.handle_vim_key("D", Some("d"), false, false, false));
    assert!(s.handle_vim_key("A", Some("a"), false, false, false));
    assert!(s.handle_vim_key("Quote", Some("\""), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "echo  end");
}

#[test]
fn vim_text_object_brackets() {
    let mut s = new_session("calculate(1 + 2) + 3");
    s.sel = Selection::caret(Pos::body(0, 12)); // inside (1 + 2)

    // 'd' + 'i' + '(' -> leaves "calculate() + 3"
    assert!(s.handle_vim_key("D", Some("d"), false, false, false));
    assert!(s.handle_vim_key("I", Some("i"), false, false, false));
    assert!(s.handle_vim_key("Paren", Some("("), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "calculate() + 3");
}

#[test]
fn vim_visual_mode_operations() {
    let mut s = new_session("Sample text line");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'v' enters Visual mode
    assert!(s.handle_vim_key("V", Some("v"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::Visual);

    // 'w' extends selection across "Sample "
    assert!(s.handle_vim_key("W", Some("w"), false, false, false));
    assert!(!s.sel.is_collapsed());

    // 'd' cuts selection -> leaves "text line", returns to Normal mode
    assert!(s.handle_vim_key("D", Some("d"), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "text line");
    assert_eq!(s.vim.mode, VimMode::Normal);
}

#[test]
fn vim_ex_command_substitute() {
    let mut s = new_session("The cat sat on the cat mat");
    s.sel = Selection::caret(Pos::body(0, 0));

    // Type ':' to enter Command mode
    assert!(s.handle_vim_key("Colon", Some(":"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::Command);
    assert_eq!(s.vim.cmd_input, ":");

    // Type `%s/cat/dog/g`
    assert!(s.handle_vim_key("Text", Some("%s/cat/dog/g"), false, false, false));
    assert_eq!(s.vim.cmd_input, ":%s/cat/dog/g");

    // Enter executes the substitute
    assert!(s.handle_vim_key("Enter", None, false, false, false));
    assert_eq!(s.vim.mode, VimMode::Normal);
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "The dog sat on the dog mat");
    assert!(s.vim.status_msg.as_deref().unwrap_or("").contains("substitutions"));
}

#[test]
fn vim_ex_command_line_navigation() {
    let mut s = new_session("Paragraph one\nParagraph two\nParagraph three");
    s.sel = Selection::caret(Pos::body(0, 0));

    // Navigate to line 2 via `:2`
    assert!(s.handle_vim_key("Colon", Some(":"), false, false, false));
    assert!(s.handle_vim_key("Text", Some("2"), false, false, false));
    assert!(s.handle_vim_key("Enter", None, false, false, false));

    assert_eq!(s.vim.mode, VimMode::Normal);
    assert_eq!(s.sel.focus.path, Path::top(1)); // 0-indexed second paragraph
}

#[test]
fn vim_search_slash_and_n() {
    let mut s = new_session("alpha beta gamma alpha delta");
    s.sel = Selection::caret(Pos::body(0, 0));

    // Type '/' to search
    assert!(s.handle_vim_key("Slash", Some("/"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::Command);
    assert_eq!(s.vim.cmd_input, "/");

    // Search for "alpha"
    assert!(s.handle_vim_key("Text", Some("alpha"), false, false, false));
    assert!(s.handle_vim_key("Enter", None, false, false, false));
    assert_eq!(s.vim.mode, VimMode::Normal);

    // 'n' moves to next match
    assert!(s.handle_vim_key("N", Some("n"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::Normal);
}

#[test]
fn vim_command_mode_backspace_and_escape() {
    let mut s = new_session("Sample");

    // Enter Command mode with ':'
    assert!(s.handle_vim_key("Colon", Some(":"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::Command);

    // Append 'w'
    assert!(s.handle_vim_key("Text", Some("w"), false, false, false));
    assert_eq!(s.vim.cmd_input, ":w");

    // Backspace erases 'w'
    assert!(s.handle_vim_key("Backspace", None, false, false, false));
    assert_eq!(s.vim.cmd_input, ":");
    assert_eq!(s.vim.mode, VimMode::Command);

    // Backspace again erases ':' and exits Command mode
    assert!(s.handle_vim_key("Backspace", None, false, false, false));
    assert_eq!(s.vim.cmd_input, "");
    assert_eq!(s.vim.mode, VimMode::Normal);

    // Escape in Command mode returns to Normal mode immediately
    assert!(s.handle_vim_key("Colon", Some(":"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::Command);
    assert!(s.handle_vim_key("Escape", None, false, false, false));
    assert_eq!(s.vim.cmd_input, "");
    assert_eq!(s.vim.mode, VimMode::Normal);
}

#[test]
fn vim_noh_clears_matches() {
    let mut s = new_session("search test");
    s.find.results = vec![(s.sel.anchor.clone(), s.sel.focus.clone())];

    assert!(s.handle_vim_key("Colon", Some(":"), false, false, false));
    assert!(s.handle_vim_key("Text", Some("noh"), false, false, false));
    assert!(s.handle_vim_key("Enter", None, false, false, false));

    assert!(s.find.results.is_empty());
}
