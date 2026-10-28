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

#[test]
fn vim_char_find_and_repeat() {
    let mut s = new_session("The quick brown fox jumps over the lazy dog.");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'f' followed by 'q' jumps to 'q' in "quick" (offset 4)
    assert!(s.handle_vim_key("F", Some("f"), false, false, false));
    assert!(s.handle_vim_key("Q", Some("q"), false, false, false));
    assert_eq!(s.sel.focus.off, 4);

    // 'f' followed by 'o' -> 'o' in "brown" (offset 12)
    s.sel = Selection::caret(Pos::body(0, 0));
    assert!(s.handle_vim_key("F", Some("f"), false, false, false));
    assert!(s.handle_vim_key("O", Some("o"), false, false, false));
    assert_eq!(s.sel.focus.off, 12);

    // ';' repeats search forward to next 'o' in "fox" (offset 17)
    assert!(s.handle_vim_key("Semicolon", Some(";"), false, false, false));
    assert_eq!(s.sel.focus.off, 17);

    // ',' repeats search backward to previous 'o' in "brown" (offset 12)
    assert!(s.handle_vim_key("Comma", Some(","), false, false, false));
    assert_eq!(s.sel.focus.off, 12);

    // 't' followed by 'b' -> caret rests right before 'b' (offset 9)
    s.sel = Selection::caret(Pos::body(0, 0));
    assert!(s.handle_vim_key("T", Some("t"), false, false, false));
    assert!(s.handle_vim_key("B", Some("b"), false, false, false));
    assert_eq!(s.sel.focus.off, 9);
}

#[test]
fn vim_visual_char_find_vf() {
    let mut s = new_session("Hello World. Test.");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'v' enters Visual mode
    assert!(s.handle_vim_key("V", Some("v"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::Visual);

    // 'f' then '.' extends selection to the first '.'
    assert!(s.handle_vim_key("F", Some("f"), false, false, false));
    assert!(s.handle_vim_key("Period", Some("."), false, false, false));
    assert_eq!(s.sel.anchor.off, 0);
    assert_eq!(s.sel.focus.off, 12); // "Hello World." is 12 chars (index 0..12)
}

#[test]
fn vim_operator_char_find_df() {
    let mut s = new_session("delete.until here");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'd' followed by 'f' and '.' deletes "delete."
    assert!(s.handle_vim_key("D", Some("d"), false, false, false));
    assert!(s.handle_vim_key("F", Some("f"), false, false, false));
    assert!(s.handle_vim_key("Period", Some("."), false, false, false));

    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "until here");
}

#[test]
fn vim_dot_repeat_commands() {
    let mut s = new_session("abcdefg");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'x' deletes 'a' -> "bcdefg"
    assert!(s.handle_vim_key("X", Some("x"), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "bcdefg");

    // '.' repeats 'x' -> deletes 'b' -> "cdefg"
    assert!(s.handle_vim_key("Period", Some("."), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "cdefg");

    // '.' repeats again -> deletes 'c' -> "defg"
    assert!(s.handle_vim_key("Period", Some("."), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "defg");
}

#[test]
fn vim_surround_operations() {
    let mut s = new_session("hello world");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'ysiw"' surrounds "hello" with quotes -> "“hello” world"
    assert!(s.handle_vim_key("Y", Some("y"), false, false, false));
    assert!(s.handle_vim_key("S", Some("s"), false, false, false));
    assert!(s.handle_vim_key("I", Some("i"), false, false, false));
    assert!(s.handle_vim_key("W", Some("w"), false, false, false));
    assert!(s.handle_vim_key("Quote", Some("\""), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "“hello” world");

    // 'cs"\'' changes surrounding quotes to single quotes -> "‘hello’ world"
    assert!(s.handle_vim_key("C", Some("c"), false, false, false));
    assert!(s.handle_vim_key("S", Some("s"), false, false, false));
    assert!(s.handle_vim_key("Quote", Some("\""), false, false, false));
    assert!(s.handle_vim_key("Quote", Some("'"), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "‘hello’ world");

    // 'ds\'' deletes surrounding single quotes -> "hello world"
    assert!(s.handle_vim_key("D", Some("d"), false, false, false));
    assert!(s.handle_vim_key("S", Some("s"), false, false, false));
    assert!(s.handle_vim_key("Quote", Some("'"), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "hello world");
}

#[test]
fn vim_flash_jump() {
    let mut s = new_session("The quick brown fox jumps");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 's' followed by 'f' and 'o' jumps to "fox" (offset 16)
    assert!(s.handle_vim_key("S", Some("s"), false, false, false));
    assert!(s.handle_vim_key("F", Some("f"), false, false, false));
    assert!(s.handle_vim_key("O", Some("o"), false, false, false));
    assert_eq!(s.sel.focus.off, 16);
}

#[test]
fn vim_visual_line_swap() {
    let mut s = new_session("First line\nSecond line");
    s.sel = Selection::caret(Pos::body(0, 0));

    // Enter VisualLine mode
    assert!(s.handle_vim_key("V", Some("V"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::VisualLine);

    // 'J' swaps current line down
    assert!(s.handle_vim_key("J", Some("J"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "Second line");
    assert_eq!(s.doc.para_at(&Pos::body(1, 0)).unwrap().text, "First line");

    // 'K' swaps current line back up
    assert!(s.handle_vim_key("K", Some("K"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "First line");
    assert_eq!(s.doc.para_at(&Pos::body(1, 0)).unwrap().text, "Second line");
}

#[test]
fn vim_leader_semicolon_and_bold() {
    let mut s = new_session("const x = 42");
    s.sel = Selection::caret(Pos::body(0, 0));

    // Space then ';' appends semicolon at the end of the line
    assert!(s.handle_vim_key("Space", Some(" "), false, false, false));
    assert_eq!(s.vim.pending, "<leader>");
    assert!(s.handle_vim_key("Semicolon", Some(";"), false, false, false));
    assert_eq!(s.doc.para_at(&s.sel.focus).unwrap().text, "const x = 42;");
}

#[test]
fn vim_viewport_centering_keys() {
    let mut s = new_session("Line for centering");
    assert_eq!(s.vim.viewport_req, None);

    // 'zz' requests center viewport
    assert!(s.handle_vim_key("Z", Some("z"), false, false, false));
    assert!(s.handle_vim_key("Z", Some("z"), false, false, false));
    assert_eq!(s.vim.viewport_req, Some("center"));

    // 'zt' requests top viewport
    assert!(s.handle_vim_key("Z", Some("z"), false, false, false));
    assert!(s.handle_vim_key("T", Some("t"), false, false, false));
    assert_eq!(s.vim.viewport_req, Some("top"));

    // 'zb' requests bottom viewport
    assert!(s.handle_vim_key("Z", Some("z"), false, false, false));
    assert!(s.handle_vim_key("B", Some("b"), false, false, false));
    assert_eq!(s.vim.viewport_req, Some("bottom"));
}

#[test]
fn vim_visual_colon_prepopulate() {
    let mut s = new_session("Sample line 1\nSample line 2");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'v' enters Visual mode
    assert!(s.handle_vim_key("V", Some("v"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::Visual);

    // ':' transitions to Command mode and pre-populates :'<,'>
    assert!(s.handle_vim_key("Colon", Some(":"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::Command);
    assert_eq!(s.vim.cmd_input, ":'<,'>");
}

#[test]
fn vim_ex_visual_range_substitution() {
    let mut s = new_session("apple pie\napple tart\napple cake");
    // Select lines 0 and 1
    s.sel = Selection { anchor: Pos::body(0, 0), focus: Pos::body(1, 10) };

    let res = crate::vim::ex::execute_ex(&mut s, ":'<,'>s/apple/cherry/g");
    assert!(!res.is_error);
    assert!(res.message.contains("2 substitutions"));

    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "cherry pie");
    assert_eq!(s.doc.para_at(&Pos::body(1, 0)).unwrap().text, "cherry tart");
    // Line 2 was outside selection, remains unchanged
    assert_eq!(s.doc.para_at(&Pos::body(2, 0)).unwrap().text, "apple cake");
}

#[test]
fn vim_visual_block_selection_and_delete() {
    let mut s = new_session("123abc\n123def\n123ghi");
    s.sel = Selection::caret(Pos::body(0, 0));

    // Ctrl+V enters VisualBlock mode
    assert!(s.handle_vim_key("V", None, true, false, false));
    assert_eq!(s.vim.mode, VimMode::VisualBlock);

    // Move 3 chars right and 2 lines down to select the 3-char wide block
    assert!(s.handle_vim_key("L", Some("l"), false, false, false));
    assert!(s.handle_vim_key("L", Some("l"), false, false, false));
    assert!(s.handle_vim_key("L", Some("l"), false, false, false));
    assert!(s.handle_vim_key("J", Some("j"), false, false, false));
    assert!(s.handle_vim_key("J", Some("j"), false, false, false));

    // 'd' deletes the block
    assert!(s.handle_vim_key("D", Some("d"), false, false, false));
    assert_eq!(s.vim.mode, VimMode::Normal);

    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "abc");
    assert_eq!(s.doc.para_at(&Pos::body(1, 0)).unwrap().text, "def");
    assert_eq!(s.doc.para_at(&Pos::body(2, 0)).unwrap().text, "ghi");
}

#[test]
fn vim_visual_block_insert_replicate() {
    let mut s = new_session("one\ntwo\nthree");
    s.sel = Selection::caret(Pos::body(0, 0));

    // Ctrl+V enters VisualBlock mode
    assert!(s.handle_vim_key("V", None, true, false, false));
    assert_eq!(s.vim.mode, VimMode::VisualBlock);

    // Select across 3 lines
    assert!(s.handle_vim_key("J", Some("j"), false, false, false));
    assert!(s.handle_vim_key("J", Some("j"), false, false, false));

    // 'I' enters Insert mode with block_edit queued
    assert!(s.handle_vim_key("I", Some("I"), false, false, true));
    assert_eq!(s.vim.mode, VimMode::Insert);
    assert!(s.vim.block_edit.is_some());

    // Type "// "
    s.vim.insert_buf.push_str("// ");
    let _ = s.run("text.insert", &json!({"text": "// "}));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "// One");

    // Escape exits Insert mode and replicates to other lines
    assert!(s.handle_vim_key("Escape", None, false, false, false));
    assert_eq!(s.vim.mode, VimMode::Normal);

    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "// One");
    assert_eq!(s.doc.para_at(&Pos::body(1, 0)).unwrap().text, "// Two");
    assert_eq!(s.doc.para_at(&Pos::body(2, 0)).unwrap().text, "// three");
}

#[test]
fn vim_visual_block_append_replicate() {
    let mut s = new_session("item1\nitem2\nitem3");
    s.sel = Selection::caret(Pos::body(0, 5));

    // Ctrl+V enters VisualBlock mode
    assert!(s.handle_vim_key("V", None, true, false, false));
    assert_eq!(s.vim.mode, VimMode::VisualBlock);

    // Select across 3 lines
    assert!(s.handle_vim_key("J", Some("j"), false, false, false));
    assert!(s.handle_vim_key("J", Some("j"), false, false, false));

    // 'A' enters Insert mode to append
    assert!(s.handle_vim_key("A", Some("A"), false, false, true));
    assert_eq!(s.vim.mode, VimMode::Insert);
    assert!(s.vim.block_edit.is_some());

    // Type ";"
    s.vim.insert_buf.push_str(";");
    let _ = s.run("text.insert", &json!({"text": ";"}));

    // Escape replicates across the lines
    assert!(s.handle_vim_key("Escape", None, false, false, false));
    assert_eq!(s.vim.mode, VimMode::Normal);

    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "item1;");
    assert_eq!(s.doc.para_at(&Pos::body(1, 0)).unwrap().text, "item2;");
    assert_eq!(s.doc.para_at(&Pos::body(2, 0)).unwrap().text, "item3;");
}

#[test]
fn vim_operator_count_motion_d3w() {
    let mut s = new_session("one two three four five");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'd' then '3' then 'w'
    assert!(s.handle_vim_key("D", Some("d"), false, false, false));
    assert_eq!(s.vim.pending_op, Some(crate::vim::VimOp::Delete));
    assert!(s.handle_vim_key("3", Some("3"), false, false, false));
    assert_eq!(s.vim.count, Some(3));
    assert!(s.handle_vim_key("W", Some("w"), false, false, false));

    // Deletes "one two three " leaving "four five"
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "four five");
}

#[test]
fn vim_replace_char_r_and_repeat() {
    let mut s = new_session("apple");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'r' then 'o' replaces 'a' with 'o' -> "opple"
    assert!(s.handle_vim_key("R", Some("r"), false, false, false));
    assert!(s.vim.replace_char_pending);
    assert!(s.handle_vim_key("O", Some("o"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "opple");

    // Move right and press '.' to repeat replace with 'o'
    assert!(s.handle_vim_key("L", Some("l"), false, false, false));
    assert!(s.handle_vim_key(".", Some("."), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "oople");
}

#[test]
fn vim_toggle_case_tilde() {
    let mut s = new_session("hELLO");
    s.sel = Selection::caret(Pos::body(0, 0));

    // '~' toggles 'h' to 'H'
    assert!(s.handle_vim_key("~", Some("~"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "HELLO");
}

#[test]
fn vim_line_end_operations_c_d_y() {
    let mut s = new_session("hello world");
    s.sel = Selection::caret(Pos::body(0, 5)); // space

    // 'D' deletes to end of line
    assert!(s.handle_vim_key("D", Some("D"), false, false, true));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "hello");

    // 'C' changes to end of line
    s.sel = Selection::caret(Pos::body(0, 2));
    assert!(s.handle_vim_key("C", Some("C"), false, false, true));
    assert_eq!(s.vim.mode, VimMode::Insert);
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "he");
    assert!(s.handle_vim_key("Escape", None, false, false, false));
}

#[test]
fn vim_substitute_line_s() {
    let mut s = new_session("hello world");
    s.sel = Selection::caret(Pos::body(0, 2));

    // 'S' clears line and enters insert mode
    assert!(s.handle_vim_key("S", Some("S"), false, false, true));
    assert_eq!(s.vim.mode, VimMode::Insert);
    assert!(s.handle_vim_key("Escape", None, false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "");
}

#[test]
fn vim_delete_char_before_x() {
    let mut s = new_session("hello");
    s.sel = Selection::caret(Pos::body(0, 4));

    // 'X' deletes char before cursor ('l')
    assert!(s.handle_vim_key("X", Some("X"), false, false, true));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "helo");
}

#[test]
fn vim_registers_named_and_black_hole() {
    let mut s = new_session("first second third");
    s.sel = Selection::caret(Pos::body(0, 0));

    // "ayw yanks "first " into register 'a'
    assert!(s.handle_vim_key("\"", Some("\""), false, false, false));
    assert!(s.handle_vim_key("A", Some("a"), false, false, false));
    assert_eq!(s.vim.current_register, Some('a'));
    assert!(s.handle_vim_key("Y", Some("y"), false, false, false));
    assert!(s.handle_vim_key("W", Some("w"), false, false, false));
    assert_eq!(s.vim.registers.get(&'a').map(String::as_str), Some("first "));

    // "_dw deletes "first " into black hole register (caret is at 0, 0)
    assert!(s.handle_vim_key("\"", Some("\""), false, false, false));
    assert!(s.handle_vim_key("_", Some("_"), false, false, false));
    assert_eq!(s.vim.current_register, Some('_'));
    assert!(s.handle_vim_key("D", Some("d"), false, false, false));
    assert!(s.handle_vim_key("W", Some("w"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "second third");

    // "ap pastes "first " from register 'a'
    assert!(s.handle_vim_key("\"", Some("\""), false, false, false));
    assert!(s.handle_vim_key("A", Some("a"), false, false, false));
    assert!(s.handle_vim_key("P", Some("p"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "first second third");
}

#[test]
fn vim_marks_and_jumps() {
    let mut s = new_session("line zero\nline one\nline two");
    s.sel = Selection::caret(Pos::body(1, 5));

    // 'ma' marks mark 'a' at (1, 5)
    assert!(s.handle_vim_key("M", Some("m"), false, false, false));
    assert!(s.handle_vim_key("A", Some("a"), false, false, false));
    assert_eq!(s.vim.marks.get(&'a').unwrap().path, Path::top(1));
    assert_eq!(s.vim.marks.get(&'a').unwrap().off, 5);

    // Move to (0, 0)
    s.sel = Selection::caret(Pos::body(0, 0));

    // '`a' jumps to exact position (1, 5)
    assert!(s.handle_vim_key("`", Some("`"), false, false, false));
    assert!(s.handle_vim_key("A", Some("a"), false, false, false));
    assert_eq!(s.sel.focus.path, Path::top(1));
    assert_eq!(s.sel.focus.off, 5);

    // '' jumps back to previous jump location (0, 0)
    assert!(s.handle_vim_key("'", Some("'"), false, false, false));
    assert!(s.handle_vim_key("'", Some("'"), false, false, false));
    assert_eq!(s.sel.focus.path, Path::top(0));
}

#[test]
fn vim_visual_swap_cursor_o() {
    let mut s = new_session("hello world");
    s.sel = Selection { anchor: Pos::body(0, 0), focus: Pos::body(0, 5) };
    s.vim.mode = VimMode::Visual;

    // 'o' swaps anchor and focus
    assert!(s.handle_vim_key("O", Some("o"), false, false, false));
    assert_eq!(s.sel.anchor, Pos::body(0, 5));
    assert_eq!(s.sel.focus, Pos::body(0, 0));
}

#[test]
fn vim_visual_case_conversion_u_and_u() {
    let mut s = new_session("Hello World");
    s.sel = Selection { anchor: Pos::body(0, 0), focus: Pos::body(0, 5) };
    s.vim.mode = VimMode::Visual;

    // 'u' lowercases selection
    assert!(s.handle_vim_key("U", Some("u"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "hello World");

    // 'U' uppercases selection
    s.sel = Selection { anchor: Pos::body(0, 6), focus: Pos::body(0, 11) };
    s.vim.mode = VimMode::Visual;
    assert!(s.handle_vim_key("U", Some("U"), false, false, true));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "hello WORLD");
}

#[test]
fn vim_bracket_match_percent() {
    let mut s = new_session("fn foo(a: i32, b: i32) { bar(); }");
    s.sel = Selection::caret(Pos::body(0, 6)); // '('

    // '%' jumps to ')'
    assert!(s.handle_vim_key("%", Some("%"), false, false, false));
    assert_eq!(s.sel.focus.off, 21); // ')'

    // '%' jumps back to '('
    assert!(s.handle_vim_key("%", Some("%"), false, false, false));
    assert_eq!(s.sel.focus.off, 6);
}

#[test]
fn vim_unicode_hex_layer_input() {
    let mut s = new_session("text ");
    s.sel = Selection::caret(Pos::body(0, 5));

    // Ctrl+Q opens unicode layer
    assert!(s.handle_vim_key("Q", None, true, false, false));
    assert!(s.vim.unicode_hex_layer.is_some());

    // Type 2, 0, 1, 4 (em-dash —)
    assert!(s.handle_vim_key("2", Some("2"), false, false, false));
    assert!(s.handle_vim_key("0", Some("0"), false, false, false));
    assert!(s.handle_vim_key("1", Some("1"), false, false, false));
    assert!(s.handle_vim_key("4", Some("4"), false, false, false));

    // Automatically inserts '—' on 4th hex digit
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "text —");
    assert!(s.vim.unicode_hex_layer.is_none());
}

#[test]
fn vim_markdown_checkbox_toggle() {
    let mut s = new_session("buy milk");
    s.sel = Selection::caret(Pos::body(0, 0));

    // <leader>x adds "- [ ] "
    assert!(s.handle_vim_key("Space", Some(" "), false, false, false));
    assert!(s.handle_vim_key("X", Some("x"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "- [ ] buy milk");

    // <leader>x toggles to "- [x] "
    assert!(s.handle_vim_key("Space", Some(" "), false, false, false));
    assert!(s.handle_vim_key("X", Some("x"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "- [x] buy milk");

    // <leader>x toggles back to "- [ ] "
    assert!(s.handle_vim_key("Space", Some(" "), false, false, false));
    assert!(s.handle_vim_key("X", Some("x"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "- [ ] buy milk");
}

#[test]
fn vim_align_equal_signs() {
    let mut s = new_session("a = 1\nlong_name = 2\nc = 3");
    s.sel = Selection::caret(Pos::body(0, 0));

    // <leader>r= aligns equal signs
    assert!(s.handle_vim_key("Space", Some(" "), false, false, false));
    assert!(s.handle_vim_key("R", Some("r"), false, false, false));
    assert!(s.handle_vim_key("=", Some("="), false, false, false));

    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "a         = 1");
    assert_eq!(s.doc.para_at(&Pos::body(1, 0)).unwrap().text, "long_name = 2");
    assert_eq!(s.doc.para_at(&Pos::body(2, 0)).unwrap().text, "c         = 3");
}

#[test]
fn vim_ex_reg_and_marks() {
    let mut s = new_session("test");
    s.vim.registers.insert('a', "stored value".to_string());
    s.vim.marks.insert('m', Pos::body(0, 2));

    let res_reg = crate::vim::ex::execute_ex(&mut s, ":reg");
    assert!(!res_reg.is_error);
    assert!(res_reg.message.contains("\"a  stored value"));

    let res_marks = crate::vim::ex::execute_ex(&mut s, ":marks");
    assert!(!res_marks.is_error);
    assert!(res_marks.message.contains("'m  line 1 col 2"));
}

#[test]
fn vim_macro_record_and_replay() {
    let mut s = new_session("foo bar baz qux");
    s.sel = Selection::caret(Pos::body(0, 0));

    // Record into register a: qa dw q
    assert!(s.handle_vim_key("Q", Some("q"), false, false, false));
    assert!(s.handle_vim_key("A", Some("a"), false, false, false));
    assert_eq!(s.vim.macro_state.recording, Some('a'));

    // Keystrokes in macro: dw
    assert!(s.handle_vim_key("D", Some("d"), false, false, false));
    assert!(s.handle_vim_key("W", Some("w"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "bar baz qux");

    // Stop recording: q
    assert!(s.handle_vim_key("Q", Some("q"), false, false, false));
    assert_eq!(s.vim.macro_state.recording, None);
    assert!(s.vim.macro_state.get('a').is_some());

    // Replay macro: @a
    assert!(s.handle_vim_key("@", Some("@"), false, false, false));
    assert!(s.handle_vim_key("A", Some("a"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "baz qux");

    // Repeat macro: @@
    assert!(s.handle_vim_key("@", Some("@"), false, false, false));
    assert!(s.handle_vim_key("@", Some("@"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "qux");
}

#[test]
fn vim_arithmetic_ctrl_a_and_ctrl_x() {
    let mut s = new_session("count = 41;");
    s.sel = Selection::caret(Pos::body(0, 0));

    // Ctrl+A increments 41 to 42
    assert!(s.handle_vim_key("A", None, true, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "count = 42;");

    // 5 Ctrl+X decrements 42 - 5 = 37
    assert!(s.handle_vim_key("5", Some("5"), false, false, false));
    assert!(s.handle_vim_key("X", None, true, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "count = 37;");
}

#[test]
fn vim_jump_list_ctrl_o_and_ctrl_i() {
    let mut s = new_session("line 1\nline 2\nline 3\nline 4");
    s.sel = Selection::caret(Pos::body(0, 0));

    // Jump to line 4 (paragraph 3)
    s.vim.jump_list.push(s.sel.focus.clone());
    s.sel = Selection::caret(Pos::body(3, 0));

    // Ctrl+O jumps back to line 1
    assert!(s.handle_vim_key("O", None, true, false, false));
    assert_eq!(s.sel.focus.path, Path::top(0));

    // Ctrl+I jumps forward to line 4
    assert!(s.handle_vim_key("I", None, true, false, false));
    assert_eq!(s.sel.focus.path, Path::top(3));
}

#[test]
fn vim_flash_authentic_labels_and_jump() {
    let mut s = new_session("apple banana apricot avocado");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 's' starts flash mode
    assert!(s.handle_vim_key("S", Some("s"), false, false, false));
    assert!(s.vim.flash_state.active);

    // type query 'ap'
    assert!(s.handle_vim_key("A", Some("a"), false, false, false));
    assert!(s.handle_vim_key("P", Some("p"), false, false, false));

    // 'ap' matches "apple" (off 0) and "apricot" (off 13)
    assert_eq!(s.vim.flash_state.targets.len(), 2);
    let target_label = s.vim.flash_state.targets[1].label;

    // Press label for second target ("apricot")
    assert!(s.handle_vim_key(&target_label.to_string(), Some(&target_label.to_string()), false, false, false));
    assert_eq!(s.sel.focus.off, 13);
    assert!(!s.vim.flash_state.active);
}

#[test]
fn vim_ex_jumps_and_changes() {
    let mut s = new_session("text");
    s.vim.jump_list.push(Pos::body(0, 1));
    s.vim.change_list.push(Pos::body(0, 3));

    let res_j = crate::vim::ex::execute_ex(&mut s, ":jumps");
    assert!(!res_j.is_error);
    assert!(res_j.message.contains("jump line  col text"));

    let res_c = crate::vim::ex::execute_ex(&mut s, ":changes");
    assert!(!res_c.is_error);
    assert!(res_c.message.contains("change line  col text"));
}

#[test]
fn vim_motions_extended_words() {
    let mut s = new_session("hello.world test_case (foo-bar)");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'e' moves to end of current word "hello" (index 4)
    assert!(s.handle_vim_key("E", Some("e"), false, false, false));
    assert_eq!(s.sel.focus.off, 4);

    // 'e' again moves to end of punctuation "." (index 5)
    assert!(s.handle_vim_key("E", Some("e"), false, false, false));
    assert_eq!(s.sel.focus.off, 5);

    // 'W' (WORD) moves to start of next whitespace-delimited WORD: "test_case" (index 12)
    assert!(s.handle_vim_key("W", Some("W"), false, false, true));
    assert_eq!(s.sel.focus.off, 12);

    // 'E' moves to end of WORD "test_case" (index 20)
    assert!(s.handle_vim_key("E", Some("E"), false, false, true));
    assert_eq!(s.sel.focus.off, 20);

    // 'B' moves backward to start of WORD "test_case" (index 12)
    assert!(s.handle_vim_key("B", Some("B"), false, false, true));
    assert_eq!(s.sel.focus.off, 12);

    // 'ge' moves to end of previous word (index 10, end of "world")
    assert!(s.handle_vim_key("G", Some("g"), false, false, false));
    assert!(s.handle_vim_key("E", Some("e"), false, false, false));
    assert_eq!(s.sel.focus.off, 10);
}

#[test]
fn vim_motions_first_non_blank_and_pipe() {
    let mut s = new_session("   leading spaces and text");
    s.sel = Selection::caret(Pos::body(0, 10));

    // '^' moves to first non-blank character (index 3)
    assert!(s.handle_vim_key("^", Some("^"), false, false, true));
    assert_eq!(s.sel.focus.off, 3);

    // '10|' jumps to column 10 (index 9, 1-indexed)
    assert!(s.handle_vim_key("1", Some("1"), false, false, false));
    assert!(s.handle_vim_key("0", Some("0"), false, false, false));
    assert!(s.handle_vim_key("|", Some("|"), false, false, true));
    assert_eq!(s.sel.focus.off, 9);
}

#[test]
fn vim_operators_shift_case_and_open() {
    let mut s = new_session("hello world");
    s.sel = Selection::caret(Pos::body(0, 0));

    // 'gUU' uppercases the line
    assert!(s.handle_vim_key("G", Some("g"), false, false, false));
    assert!(s.handle_vim_key("U", Some("U"), false, false, true));
    assert!(s.handle_vim_key("U", Some("U"), false, false, true));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "HELLO WORLD");

    // 'guu' lowercases the line
    assert!(s.handle_vim_key("G", Some("g"), false, false, false));
    assert!(s.handle_vim_key("U", Some("u"), false, false, false));
    assert!(s.handle_vim_key("U", Some("u"), false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "hello world");

    // 'I' moves to first non-blank and enters Insert mode
    s.sel = Selection::caret(Pos::body(0, 6));
    assert!(s.handle_vim_key("I", Some("I"), false, false, true));
    assert_eq!(s.vim.mode, VimMode::Insert);
    assert_eq!(s.sel.focus.off, 0);

    // Escape back to Normal
    assert!(s.handle_vim_key("Escape", None, false, false, false));
    assert_eq!(s.vim.mode, VimMode::Normal);
}

#[test]
fn vim_text_objects_extended() {
    // 1. diW on whitespace-delimited WORD
    let mut s = new_session("foo foo.bar.baz baz");
    s.sel = Selection::caret(Pos::body(0, 6)); // in "foo.bar.baz"
    assert!(s.handle_vim_key("D", Some("d"), false, false, false));
    assert!(s.handle_vim_key("I", Some("i"), false, false, false));
    assert!(s.handle_vim_key("W", Some("W"), false, false, true));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "foo  baz");

    // 2. Forward scan for quotes: di" from outside quotes
    let mut s2 = new_session("hello \"world\" test");
    s2.sel = Selection::caret(Pos::body(0, 0)); // at 'h', outside quotes
    assert!(s2.handle_vim_key("D", Some("d"), false, false, false));
    assert!(s2.handle_vim_key("I", Some("i"), false, false, false));
    assert!(s2.handle_vim_key("\"", Some("\""), false, false, false));
    assert_eq!(s2.doc.para_at(&Pos::body(0, 0)).unwrap().text, "hello \"\" test");

    // 3. HTML/XML tags: dit and dat
    let mut s3 = new_session("<div class=\"box\">content here</div>");
    s3.sel = Selection::caret(Pos::body(0, 20)); // in "content here"
    assert!(s3.handle_vim_key("D", Some("d"), false, false, false));
    assert!(s3.handle_vim_key("I", Some("i"), false, false, false));
    assert!(s3.handle_vim_key("T", Some("t"), false, false, false));
    assert_eq!(s3.doc.para_at(&Pos::body(0, 0)).unwrap().text, "<div class=\"box\"></div>");
}

#[test]
fn vim_registers_linewise_and_append() {
    // 1. Linewise yank & paste
    let mut s = new_session("Line One\nLine Two");
    s.sel = Selection::caret(Pos::body(0, 2));

    // 'yy' yanks entire Line One
    assert!(s.handle_vim_key("Y", Some("y"), false, false, false));
    assert!(s.handle_vim_key("Y", Some("y"), false, false, false));

    // 'p' pastes on a new line below
    assert!(s.handle_vim_key("P", Some("p"), false, false, false));
    assert_eq!(s.doc.body.len(), 3);
    assert_eq!(s.doc.para_at(&Pos::body(1, 0)).unwrap().text, "Line One");

    // 2. Register append with uppercase: "ayw then "Ayw
    let mut s2 = new_session("alpha beta gamma");
    s2.sel = Selection::caret(Pos::body(0, 0));

    // "ayw yanks "alpha " into register 'a'
    assert!(s2.handle_vim_key("\"", Some("\""), false, false, false));
    assert!(s2.handle_vim_key("A", Some("a"), false, false, false));
    assert!(s2.handle_vim_key("Y", Some("y"), false, false, false));
    assert!(s2.handle_vim_key("W", Some("w"), false, false, false));
    assert_eq!(s2.vim.registers.get(&'a').map(String::as_str), Some("alpha "));

    // Move to "beta "
    s2.sel = Selection::caret(Pos::body(0, 6));

    // "Ayw appends "beta " into register 'a'
    assert!(s2.handle_vim_key("\"", Some("\""), false, false, false));
    assert!(s2.handle_vim_key("A", Some("A"), false, false, true));
    assert!(s2.handle_vim_key("Y", Some("y"), false, false, false));
    assert!(s2.handle_vim_key("W", Some("w"), false, false, false));
    assert_eq!(s2.vim.registers.get(&'a').map(String::as_str), Some("alpha beta "));
}

#[test]
fn vim_replace_mode_and_backspace() {
    let mut s = new_session("abcdef");
    s.sel = Selection::caret(Pos::body(0, 1)); // at 'b'

    // 'R' enters Replace mode
    assert!(s.handle_vim_key("R", Some("R"), false, false, true));
    assert_eq!(s.vim.mode, VimMode::Replace);

    // Overwrite 'b' with 'X'
    assert!(s.handle_vim_key("X", Some("X"), false, false, true));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "aXcdef");

    // Overwrite 'c' with 'Y'
    assert!(s.handle_vim_key("Y", Some("Y"), false, false, true));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "aXYdef");

    // Backspace restores 'c'
    assert!(s.handle_vim_key("Backspace", None, false, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "aXcdef");

    // Escape exits Replace mode
    assert!(s.handle_vim_key("Escape", None, false, false, false));
    assert_eq!(s.vim.mode, VimMode::Normal);
}

#[test]
fn vim_insert_mode_chords_and_one_shot() {
    // 1. <C-w> deletes previous word in insert mode
    let mut s = new_session("hello world ");
    s.vim.mode = VimMode::Insert;
    s.sel = Selection::caret(Pos::body(0, 12));

    assert!(s.handle_vim_key("W", None, true, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "hello ");

    // 2. <C-u> deletes to start of line
    assert!(s.handle_vim_key("U", None, true, false, false));
    assert_eq!(s.doc.para_at(&Pos::body(0, 0)).unwrap().text, "");

    // 3. <C-o> one-shot normal mode
    let mut s2 = new_session("first line\nsecond line");
    s2.vim.mode = VimMode::Insert;
    s2.sel = Selection::caret(Pos::body(0, 0));

    // Press <C-o>
    assert!(s2.handle_vim_key("O", None, true, false, false));
    assert_eq!(s2.vim.mode, VimMode::Normal);
    assert!(s2.vim.one_shot_normal);

    // Execute normal motion 'j'
    assert!(s2.handle_vim_key("J", Some("j"), false, false, false));
    // Immediately returns to Insert mode!
    assert_eq!(s2.vim.mode, VimMode::Insert);
    assert!(!s2.vim.one_shot_normal);
    assert_eq!(s2.sel.focus.path, Path::top(1));
}
