//! Native modal Vim editing engine for WordCraft.
//!
//! Provides a pure-Rust modal state machine (Normal, Insert, Visual, VisualLine, Command)
//! that maps Vim motions, operators, text objects, and counts directly into WordCraft's command registry.

pub mod config;
pub mod ex;
pub mod flash;
pub mod insert;
pub mod jump_list;
pub mod macro_rec;
pub mod marks;
pub mod motion;
pub mod registers;
pub mod repeat;
pub mod surround;
pub mod text_object;
pub mod unicode;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Selection, Session};
pub use config::VimConfig;
pub use marks::MarkTable;
pub use motion::CharFind;
pub use registers::{RegType, RegisterBank, RegisterValue};
pub use repeat::LastVimChange;

/// An operator waiting for a motion or text object (`d`, `c`, `y`, `>`, `<`, `=`, `g~`, `gu`, `gU`, `gq`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VimOp {
    /// Delete / cut range.
    Delete,
    /// Change range (delete + enter Insert mode).
    Change,
    /// Yank / copy range to clipboard.
    Yank,
    /// Shift right / indent.
    ShiftRight,
    /// Shift left / outdent.
    ShiftLeft,
    /// Auto-indent / reformat.
    Indent,
    /// Toggle case over range.
    ToggleCase,
    /// Lowercase range.
    LowerCase,
    /// Uppercase range.
    UpperCase,
    /// Format / wrap paragraph.
    Format,
}

/// The active modal state in Vim mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VimMode {
    /// Normal navigation and manipulation mode.
    #[default]
    Normal,
    /// Standard typing into the document.
    Insert,
    /// Character overwrite mode (`R`).
    Replace,
    /// Character-wise visual selection.
    Visual,
    /// Line/paragraph-wise visual selection.
    VisualLine,
    /// Rectangular column block visual selection.
    VisualBlock,
    /// Ex-command line mode (`:`).
    Command,
}

impl VimMode {
    /// Short display badge for the status bar.
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Normal => "-- NORMAL --",
            Self::Insert => "-- INSERT --",
            Self::Replace => "-- REPLACE --",
            Self::Visual => "-- VISUAL --",
            Self::VisualLine => "-- VISUAL LINE --",
            Self::VisualBlock => "-- VISUAL BLOCK --",
            Self::Command => "-- COMMAND --",
        }
    }
}

/// State for replicating a visual block edit across multiple lines on Escape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockEditState {
    /// Paragraph indices for the other lines in the block.
    pub lines: Vec<usize>,
    /// Column byte offset in each paragraph.
    pub col: usize,
    /// Whether this is appending (after column) or inserting (at/before column).
    pub is_append: bool,
}

/// Persistent and runtime state for Vim mode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VimState {
    /// Whether Vim mode is enabled.
    pub enabled: bool,
    /// Current modal state.
    pub mode: VimMode,
    /// Multiplier count accumulator (e.g. `5` in `5j`).
    pub count: Option<usize>,
    /// Pending key sequence waiting for completion (e.g. `g` waiting for `g`, or `di` waiting for `w`).
    pub pending: String,
    /// Pending operator (`d`, `c`, `y`).
    pub pending_op: Option<VimOp>,
    /// Active configuration.
    pub config: VimConfig,
    /// Pending command line input (in Command mode, e.g. `:w` or `/pattern`).
    pub cmd_input: String,
    /// Optional status or feedback message.
    pub status_msg: Option<String>,
    /// Last character find operation for repeatability with `;` and `,`.
    pub last_char_find: Option<CharFind>,
    /// Last modifying edit for dot repeat (`.`).
    pub last_change: Option<LastVimChange>,
    /// Viewport scroll target request (e.g. "center", "top", "bottom").
    pub viewport_req: Option<&'static str>,
    /// Multiplier count preceding an operator (e.g. `2` in `2dw`).
    pub op_count: Option<usize>,
    /// Insert text buffer recorded during Insert mode for repeating change operations.
    pub insert_buf: String,
    /// Active multi-line block edit in progress.
    pub block_edit: Option<BlockEditState>,
    /// Complete 39-register subsystem (a-z, A-Z, 0-9, +, *, _, ., :, %, /, =).
    pub registers: RegisterBank,
    /// Currently selected register prefix (e.g. `"a` sets this to Some('a')).
    pub current_register: Option<char>,
    /// Named marks (a-z, A-Z, special marks) mapping to document positions.
    pub marks: MarkTable,
    /// Stack of overwritten characters during Replace mode for backspace restore.
    pub replace_stack: Vec<(wordcraft_doc::Pos, String)>,
    /// One-shot normal mode flag for `<C-o>` in Insert mode.
    pub one_shot_normal: bool,
    /// Position before the last jump (for '' and ``).
    pub last_jump: Option<wordcraft_doc::Pos>,
    /// Unicode hex input layer buffer (active when Some).
    pub unicode_hex_layer: Option<String>,
    /// Last visual selection for `gv` re-select.
    pub last_visual_sel: Option<Selection>,
    /// Waiting for a character after `r` command.
    pub replace_char_pending: bool,
    /// Macro recording and playback state.
    pub macro_state: macro_rec::VimMacroState,
    /// Jump list history (<C-o>/<C-i>).
    pub jump_list: jump_list::JumpList,
    /// Change list history (g;/g,).
    pub change_list: jump_list::ChangeList,
    /// Active flash.nvim jump state.
    pub flash_state: flash::FlashState,
}

impl Default for VimState {
    fn default() -> Self {
        let config = VimConfig::load();
        let mode = if config.default_mode == "insert" { VimMode::Insert } else { VimMode::Normal };
        Self {
            enabled: false,
            mode,
            count: None,
            op_count: None,
            pending: String::new(),
            pending_op: None,
            config,
            cmd_input: String::new(),
            status_msg: None,
            last_char_find: None,
            last_change: None,
            viewport_req: None,
            insert_buf: String::new(),
            block_edit: None,
            registers: RegisterBank::default(),
            current_register: None,
            marks: MarkTable::default(),
            replace_stack: Vec::new(),
            one_shot_normal: false,
            last_jump: None,
            unicode_hex_layer: None,
            last_visual_sel: None,
            replace_char_pending: false,
            macro_state: macro_rec::VimMacroState::default(),
            jump_list: jump_list::JumpList::default(),
            change_list: jump_list::ChangeList::default(),
            flash_state: flash::FlashState::default(),
        }
    }
}

impl VimState {
    /// Create a new Vim state with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle Vim mode on or off.
    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        if self.enabled {
            self.mode = if self.config.default_mode == "insert" { VimMode::Insert } else { VimMode::Normal };
            self.reset_pending();
        }
        self.enabled
    }

    /// Set the current mode directly.
    pub fn set_mode(&mut self, mode: VimMode) {
        self.mode = mode;
        self.reset_pending();
    }

    /// Clear pending key sequences and operators.
    pub fn reset_pending(&mut self) {
        self.pending.clear();
        self.pending_op = None;
        self.count = None;
        self.op_count = None;
        self.cmd_input.clear();
        self.replace_char_pending = false;
        self.current_register = None;
        self.flash_state.cancel();
    }

    /// Take and clear the accumulated count (defaults to 1 if none).
    pub fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1).clamp(1, 1000)
    }

    /// Handle a keystroke. Returns true if the key was consumed by Vim mode.
    pub fn handle_key(&mut self, session: &mut Session, key_name: &str, text: Option<&str>, ctrl: bool, alt: bool, shift: bool) -> bool {
        if !self.enabled {
            return false;
        }

        // Record keystroke if macro recording is active and not currently replaying
        if self.macro_state.recording.is_some() && !self.macro_state.is_replaying {
            let is_stop_q = self.mode == VimMode::Normal && key_name == "Q" && !ctrl && !alt && self.pending.is_empty();
            if !is_stop_q {
                self.macro_state.record(key_name, text, ctrl, alt, shift);
            }
        }

        let was_normal = self.mode == VimMode::Normal;
        let handled = match self.mode {
            VimMode::Insert => insert::handle_insert_key(self, session, key_name, text, ctrl, alt, shift),
            VimMode::Replace => insert::handle_replace_key(self, session, key_name, text),
            VimMode::Normal => self.handle_normal(session, key_name, text, ctrl, alt, shift),
            VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock => self.handle_visual(session, key_name, text, ctrl, shift),
            VimMode::Command => self.handle_command(session, key_name, text),
        };

        if was_normal && self.one_shot_normal && self.mode == VimMode::Normal && self.pending.is_empty() && self.pending_op.is_none() {
            self.one_shot_normal = false;
            self.mode = VimMode::Insert;
            self.status_msg = None;
        }

        handled
    }

    fn handle_normal(&mut self, session: &mut Session, key_name: &str, text: Option<&str>, ctrl: bool, alt: bool, _shift: bool) -> bool {
        // Escape clears pending sequence, count, operators, and selections
        if key_name == "Escape" {
            self.reset_pending();
            self.status_msg = None;
            let _ = session.run("select.collapse", &json!({}));
            return true;
        }

        let input_char = text.and_then(|t| t.chars().next());

        // Unicode hex layer
        if let Some(mut hex_buf) = self.unicode_hex_layer.clone() {
            let result = unicode::handle_hex_key(session, &mut hex_buf, key_name, input_char);
            match result {
                unicode::HexKeyResult::Continue => {
                    self.unicode_hex_layer = Some(hex_buf.clone());
                    self.status_msg = Some(unicode::format_display(&hex_buf));
                    return true;
                }
                unicode::HexKeyResult::Inserted => {
                    self.unicode_hex_layer = None;
                    self.status_msg = None;
                    return true;
                }
                unicode::HexKeyResult::Cancel => {
                    self.unicode_hex_layer = None;
                    self.status_msg = None;
                    return true;
                }
            }
        }

        if ctrl && !alt && key_name == "Q" {
            self.unicode_hex_layer = Some(String::new());
            self.status_msg = Some(unicode::format_display(""));
            return true;
        }

        // Replace char pending: r{char}
        if self.replace_char_pending {
            if let Some(c) = input_char {
                let cnt = self.take_count();
                for _ in 0..cnt {
                    let _ = session.run("text.delete", &json!({}));
                    let _ = session.run("text.insert", &json!({"text": c.to_string()}));
                    let _ = session.run("caret.left", &json!({}));
                }
                self.last_change = Some(LastVimChange::ReplaceChar(c));
            }
            self.replace_char_pending = false;
            return true;
        }

        // Register prefix: "{reg}
        if self.pending == "\"" {
            match input_char {
                Some(c)
                    if c.is_ascii_lowercase()
                        || c.is_ascii_uppercase()
                        || c.is_ascii_digit()
                        || matches!(c, '+' | '*' | '_' | '-' | '.' | ':' | '%' | '/' | '=') =>
                {
                    self.current_register = Some(c);
                }
                _ => {}
            }
            self.pending.clear();
            return true;
        }

        // Mark operations: m{mark}, '{mark}, `{mark}
        if self.pending == "m" {
            if let Some(c) = input_char
                && c.is_ascii_alphanumeric()
            {
                self.marks.set(c, session.sel.focus.clone());
                self.status_msg = Some(format!("mark '{c}' set"));
            }
            self.pending.clear();
            return true;
        }

        if self.pending == "'" || self.pending == "`" {
            let exact = self.pending == "`";
            if let Some(c) = input_char {
                if c == '\'' || c == '`' {
                    if let Some(pos) = self.marks.last_jump.clone().or_else(|| self.last_jump.clone()) {
                        let cur = session.sel.focus.clone();
                        session.sel = Selection::caret(pos);
                        self.marks.last_jump = Some(cur.clone());
                        self.last_jump = Some(cur);
                    }
                } else if let Some(pos) = self.marks.get(&c) {
                    let cur = session.sel.focus.clone();
                    self.marks.last_jump = Some(cur.clone());
                    self.last_jump = Some(cur);
                    if exact {
                        session.sel = Selection::caret(pos);
                    } else {
                        let p_idx = pos.path.0.first().copied().unwrap_or(0) as usize;
                        let off = session.doc.para_at(&pos).map(|p| motion::first_non_blank(&p.text)).unwrap_or(0);
                        session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, off));
                    }
                } else {
                    self.status_msg = Some(format!("E20: Mark not set: {c}"));
                }
            }
            self.pending.clear();
            return true;
        }

        // Flash jump interactive mode
        if self.pending == "flash" {
            if key_name == "Escape" {
                self.reset_pending();
                self.status_msg = None;
                return true;
            }
            if let Some(c) = input_char {
                if self.flash_state.query.len() < 2 {
                    self.flash_state.query.push(c);
                    self.status_msg = Some(format!("flash: {}", self.flash_state.query));
                    let jumped = flash::update_flash_targets(session, &mut self.flash_state);
                    if jumped {
                        self.reset_pending();
                        self.status_msg = None;
                    }
                    return true;
                } else {
                    if let Some(target_pos) = self.flash_state.target_for_label(c) {
                        self.jump_list.push(session.sel.focus.clone());
                        session.sel = Selection::caret(target_pos);
                    }
                    self.reset_pending();
                    self.status_msg = None;
                    return true;
                }
            }
            return true;
        }

        // Start macro recording: q{reg}
        if self.pending == "q" {
            if let Some(c) = input_char.filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
                self.macro_state.start_recording(c);
                self.status_msg = Some(format!("recording @{c}"));
            }
            self.pending.clear();
            return true;
        }

        // Play macro: @{reg} or @@
        if self.pending == "@" {
            if let Some(c) = input_char {
                let target_reg = if c == '@' {
                    self.macro_state.last_executed
                } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
                    Some(c)
                } else {
                    None
                };
                if let Some(reg) = target_reg {
                    self.macro_state.last_executed = Some(reg);
                    if let Some(keys) = self.macro_state.get(reg).map(|k| k.to_vec()) {
                        let cnt = self.take_count();
                        self.pending.clear();
                        self.macro_state.is_replaying = true;
                        for _ in 0..cnt {
                            for k in &keys {
                                self.handle_key(session, &k.key_name, k.text.as_deref(), k.ctrl, k.alt, k.shift);
                            }
                        }
                        self.macro_state.is_replaying = false;
                        return true;
                    }
                }
            }
            self.pending.clear();
            return true;
        }

        // Window commands: <C-w>{key}
        if self.pending == "<C-w>" {
            match input_char {
                Some('v') => {
                    let _ = session.run("view.navigationPane", &json!({}));
                }
                Some('s') => {
                    let _ = session.run("view.comments", &json!({}));
                }
                Some('w') => { /* cycle pane */ }
                Some('c') | Some('q') => {
                    let _ = session.run("ui.closePane", &json!({}));
                }
                Some('o') => {
                    let _ = session.run("view.navigationPane", &json!({"value": false}));
                    let _ = session.run("view.comments", &json!({"value": false}));
                }
                _ => {}
            }
            self.pending.clear();
            return true;
        }

        // Numeric count accumulator (1-9, or 0 if count already started)
        if !ctrl
            && !alt
            && self.pending.is_empty()
            && !self.replace_char_pending
            && let Some(c) = input_char
            && c.is_ascii_digit()
            && (c != '0' || self.count.is_some())
        {
            let digit = (c as usize).saturating_sub('0' as usize);
            let current = self.count.unwrap_or(0);
            self.count = Some(current.saturating_mul(10).saturating_add(digit).min(9999));
            return true;
        }

        // Leader key check
        if !self.pending.is_empty() && self.pending.starts_with("<leader>") {
            if let Some(c) = input_char {
                let mut full = self.pending.clone();
                full.push(c);
                if let Some(target) = self.config.normal_mappings.get(&full).cloned() {
                    self.execute_mapping(session, &target);
                    self.reset_pending();
                    return true;
                }
                if self.pending == "<leader>" {
                    match c {
                        ';' => {
                            let _ = session.run("caret.end", &json!({}));
                            let _ = session.run("text.insert", &json!({"text": ";"}));
                            self.reset_pending();
                            return true;
                        }
                        'b' => {
                            let _ = session.run("select.word", &json!({}));
                            let _ = session.run("format.bold", &json!({}));
                            self.reset_pending();
                            return true;
                        }
                        'i' => {
                            let _ = session.run("select.word", &json!({}));
                            let _ = session.run("format.italic", &json!({}));
                            self.reset_pending();
                            return true;
                        }
                        's' => {
                            let _ = session.run("select.word", &json!({}));
                            let _ = session.run("format.strikethrough", &json!({}));
                            self.reset_pending();
                            return true;
                        }
                        'h' => {
                            let _ = session.run("select.word", &json!({}));
                            let _ = session.run("format.highlight", &json!({}));
                            self.reset_pending();
                            return true;
                        }
                        'c' => {
                            let _ = session.run("select.word", &json!({}));
                            let _ = session.run("format.code", &json!({}));
                            self.reset_pending();
                            return true;
                        }
                        '1' => {
                            let _ = session.run("style.heading1", &json!({}));
                            self.reset_pending();
                            return true;
                        }
                        '2' => {
                            let _ = session.run("style.heading2", &json!({}));
                            self.reset_pending();
                            return true;
                        }
                        '3' => {
                            let _ = session.run("style.heading3", &json!({}));
                            self.reset_pending();
                            return true;
                        }
                        '4' => {
                            let _ = session.run("style.heading4", &json!({}));
                            self.reset_pending();
                            return true;
                        }
                        '5' => {
                            let _ = session.run("style.heading5", &json!({}));
                            self.reset_pending();
                            return true;
                        }
                        '6' => {
                            let _ = session.run("style.heading6", &json!({}));
                            self.reset_pending();
                            return true;
                        }
                        'x' => {
                            // Toggle markdown task checkbox: - [ ] <-> - [x]
                            let focus = &session.sel.focus;
                            if let Some(para) = session.doc.para_at(focus) {
                                let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                                let txt = &para.text;
                                if txt.starts_with("- [x] ") || txt.starts_with("- [X] ") {
                                    let pa = wordcraft_doc::Pos::body(p_idx, 3);
                                    let pb = wordcraft_doc::Pos::body(p_idx, 4);
                                    let props = para.props_of_char(3).clone();
                                    if let Ok(at) = session.doc.delete_range(&pa, &pb) {
                                        let _ = session.doc.insert_text(&at, " ", &props);
                                    }
                                    session.relayout();
                                } else if txt.starts_with("- [ ] ") {
                                    let pa = wordcraft_doc::Pos::body(p_idx, 3);
                                    let pb = wordcraft_doc::Pos::body(p_idx, 4);
                                    let props = para.props_of_char(3).clone();
                                    if let Ok(at) = session.doc.delete_range(&pa, &pb) {
                                        let _ = session.doc.insert_text(&at, "x", &props);
                                    }
                                    session.relayout();
                                } else {
                                    let pa = wordcraft_doc::Pos::body(p_idx, 0);
                                    let props = para.props_of_char(0).clone();
                                    let _ = session.doc.insert_text(&pa, "- [ ] ", &props);
                                    session.relayout();
                                }
                            }
                            self.reset_pending();
                            return true;
                        }
                        'r' => {
                            self.pending = "<leader>r".to_string();
                            return true;
                        }
                        'm' => {
                            self.pending = "<leader>m".to_string();
                            return true;
                        }
                        _ => {}
                    }
                } else if self.pending == "<leader>r" {
                    match c {
                        '=' => {
                            align_equal_signs(session);
                        }
                        's' => {
                            let focus = &session.sel.focus;
                            if let Some(para) = session.doc.para_at(focus) {
                                let word = extract_word_at(&para.text, focus.off);
                                if !word.is_empty() {
                                    self.mode = VimMode::Command;
                                    self.cmd_input = format!(":%s/{word}//g");
                                }
                            }
                        }
                        _ => {}
                    }
                    self.reset_pending();
                    return true;
                } else if self.pending == "<leader>m" {
                    match c {
                        'w' => {
                            let mut words = 0;
                            let mut chars = 0;
                            let paras = session.doc.body.len();
                            for p in &session.doc.body {
                                if let Some(para) = p.as_para() {
                                    words += para.text.split_whitespace().count();
                                    chars += para.text.chars().count();
                                }
                            }
                            self.status_msg = Some(format!("{words} words, {chars} chars, {paras} paras"));
                        }
                        'r' => {
                            let _ = session.run("format.clearFormatting", &json!({}));
                        }
                        _ => {}
                    }
                    self.reset_pending();
                    return true;
                }
            }
            self.reset_pending();
            return true;
        }

        // Check if starting a leader sequence
        if self.pending_op.is_none()
            && let Some(c) = input_char
            && self.config.leader == c.to_string()
        {
            self.pending = "<leader>".to_string();
            return true;
        }

        // Ctrl shortcuts in Normal mode
        if ctrl && !alt && self.pending_op.is_none() {
            match key_name {
                "D" => {
                    let _ = session.run("caret.pageDown", &json!({}));
                    self.viewport_req = Some("center");
                    self.count = None;
                    return true;
                }
                "U" => {
                    let _ = session.run("caret.pageUp", &json!({}));
                    self.viewport_req = Some("center");
                    self.count = None;
                    return true;
                }
                "R" => {
                    let _ = session.run("edit.redo", &json!({}));
                    self.count = None;
                    return true;
                }
                "V" => {
                    self.mode = VimMode::VisualBlock;
                    let _ = session.run("select.column", &json!({"anchor": session.sel.anchor, "focus": session.sel.focus}));
                    self.count = None;
                    return true;
                }
                "Q" => {
                    self.unicode_hex_layer = Some(String::new());
                    self.status_msg = Some(unicode::format_display(""));
                    self.count = None;
                    return true;
                }
                "A" => {
                    return self.handle_arithmetic(session, 1);
                }
                "X" => {
                    return self.handle_arithmetic(session, -1);
                }
                "O" => {
                    if let Some(pos) = self.jump_list.jump_older(&session.sel.focus) {
                        session.sel = Selection::caret(pos);
                        return true;
                    }
                }
                "I" => {
                    if let Some(pos) = self.jump_list.jump_newer() {
                        session.sel = Selection::caret(pos);
                        return true;
                    }
                }
                "E" => {
                    self.viewport_req = Some("down_line");
                    return true;
                }
                "Y" => {
                    self.viewport_req = Some("up_line");
                    return true;
                }
                "F" => {
                    let _ = session.run("caret.pageDown", &json!({}));
                    return true;
                }
                "B" => {
                    let _ = session.run("caret.pageUp", &json!({}));
                    return true;
                }
                "W" => {
                    self.pending = "<C-w>".to_string();
                    return true;
                }
                _ => {}
            }
        }

        // Operator pending mode: d, c, y
        if let Some(op) = self.pending_op {
            return self.handle_operator_pending(session, op, key_name, input_char);
        }

        // Multi-character pending sequences
        if !self.pending.is_empty() {
            let p = self.pending.clone();
            if p == "g" {
                match input_char {
                    Some('g') => {
                        if let Some(line_num) = self.count.take() {
                            let para_idx = line_num.saturating_sub(1).min(session.doc.body.len().saturating_sub(1));
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(para_idx, 0));
                        } else {
                            let _ = session.run("caret.docStart", &json!({}));
                        }
                        self.reset_pending();
                        return true;
                    }
                    Some('e') => {
                        for _ in 0..self.take_count() {
                            let focus = session.sel.focus.clone();
                            let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                            if let Some(para) = session.doc.para_at(&focus) {
                                let prev_off = motion::word_end_backward(&para.text, focus.off);
                                session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, prev_off));
                            }
                        }
                        self.reset_pending();
                        return true;
                    }
                    Some('E') => {
                        for _ in 0..self.take_count() {
                            let focus = session.sel.focus.clone();
                            let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                            if let Some(para) = session.doc.para_at(&focus) {
                                let prev_off = motion::big_word_end_backward(&para.text, focus.off);
                                session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, prev_off));
                            }
                        }
                        self.reset_pending();
                        return true;
                    }
                    Some('v') => {
                        if let Some(sel) = self.last_visual_sel.clone() {
                            session.sel = sel;
                            self.mode = VimMode::Visual;
                        }
                        self.reset_pending();
                        return true;
                    }
                    Some('l') => {
                        let focus = &session.sel.focus;
                        if let Some(para) = session.doc.para_at(focus) {
                            if let Some(link) = para.props_of_char(focus.off).link.clone() {
                                self.status_msg = Some(format!("Link: {link}"));
                            } else {
                                self.status_msg = Some("No link at cursor".to_string());
                            }
                        }
                        self.reset_pending();
                        return true;
                    }
                    Some(';') => {
                        if let Some(pos) = self.change_list.jump_older() {
                            session.sel = Selection::caret(pos);
                        }
                        self.reset_pending();
                        return true;
                    }
                    Some(',') => {
                        if let Some(pos) = self.change_list.jump_newer() {
                            session.sel = Selection::caret(pos);
                        }
                        self.reset_pending();
                        return true;
                    }
                    Some('_') => {
                        let focus = session.sel.focus.clone();
                        let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                        if let Some(para) = session.doc.para_at(&focus) {
                            let off = motion::last_non_blank(&para.text);
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, off));
                        }
                        self.reset_pending();
                        return true;
                    }
                    Some('~') => {
                        self.op_count = self.count.take();
                        self.pending_op = Some(VimOp::ToggleCase);
                        self.pending.clear();
                        return true;
                    }
                    Some('u') => {
                        self.op_count = self.count.take();
                        self.pending_op = Some(VimOp::LowerCase);
                        self.pending.clear();
                        return true;
                    }
                    Some('U') => {
                        self.op_count = self.count.take();
                        self.pending_op = Some(VimOp::UpperCase);
                        self.pending.clear();
                        return true;
                    }
                    Some('q') | Some('w') => {
                        self.op_count = self.count.take();
                        self.pending_op = Some(VimOp::Format);
                        self.pending.clear();
                        return true;
                    }
                    Some('J') => {
                        let _ = session.run("caret.end", &json!({}));
                        let _ = session.run("text.delete", &json!({}));
                        self.reset_pending();
                        return true;
                    }
                    _ => {}
                }
                self.reset_pending();
                return true;
            }
            if p == "[" {
                if input_char == Some('[') {
                    for _ in 0..self.take_count() {
                        let pos = motion::section_jump(session, &session.sel.focus, false);
                        session.sel = Selection::caret(pos);
                    }
                }
                self.reset_pending();
                return true;
            }
            if p == "]" {
                if input_char == Some(']') {
                    for _ in 0..self.take_count() {
                        let pos = motion::section_jump(session, &session.sel.focus, true);
                        session.sel = Selection::caret(pos);
                    }
                }
                self.reset_pending();
                return true;
            }
            if p == "z" {
                match input_char {
                    Some('z') => {
                        self.viewport_req = Some("center");
                    }
                    Some('t') => {
                        self.viewport_req = Some("top");
                    }
                    Some('b') => {
                        self.viewport_req = Some("bottom");
                    }
                    _ => {}
                }
                self.reset_pending();
                return true;
            }
            if matches!(p.as_str(), "f" | "F" | "t" | "T") {
                if let Some(target) = input_char {
                    let forward = p == "f" || p == "t";
                    let till = p == "t" || p == "T";
                    let find = motion::CharFind { target, forward, till };
                    self.last_char_find = Some(find);
                    motion::execute_char_find(session, find, false);
                }
                self.reset_pending();
                return true;
            }

            self.reset_pending();
            return true;
        }

        // Initiating operators: d, c, y, >, <, =
        match input_char {
            Some('d') => {
                self.op_count = self.count.take();
                self.pending_op = Some(VimOp::Delete);
                return true;
            }
            Some('c') => {
                self.op_count = self.count.take();
                self.pending_op = Some(VimOp::Change);
                return true;
            }
            Some('y') => {
                self.op_count = self.count.take();
                self.pending_op = Some(VimOp::Yank);
                return true;
            }
            Some('>') => {
                self.op_count = self.count.take();
                self.pending_op = Some(VimOp::ShiftRight);
                return true;
            }
            Some('<') => {
                self.op_count = self.count.take();
                self.pending_op = Some(VimOp::ShiftLeft);
                return true;
            }
            Some('=') => {
                self.op_count = self.count.take();
                self.pending_op = Some(VimOp::Indent);
                return true;
            }
            _ => {}
        }

        // Single-character commands and motions
        let cnt = self.take_count();
        match input_char {
            Some('h') => {
                repeat(cnt, || session.run("caret.left", &json!({})));
                true
            }
            Some('l') => {
                repeat(cnt, || session.run("caret.right", &json!({})));
                true
            }
            Some('j') => {
                repeat(cnt, || session.run("caret.down", &json!({})));
                true
            }
            Some('k') => {
                repeat(cnt, || session.run("caret.up", &json!({})));
                true
            }
            Some('w') => {
                for _ in 0..cnt {
                    let focus = session.sel.focus.clone();
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    if let Some(para) = session.doc.para_at(&focus) {
                        let next_off = motion::word_forward(&para.text, focus.off);
                        if next_off < para.len() || p_idx + 1 >= session.doc.body.len() {
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, next_off));
                        } else {
                            let next_p = p_idx + 1;
                            let off =
                                session.doc.para_at(&wordcraft_doc::Pos::body(next_p, 0)).map(|p| motion::first_non_blank(&p.text)).unwrap_or(0);
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(next_p, off));
                        }
                    }
                }
                true
            }
            Some('b') => {
                for _ in 0..cnt {
                    let focus = session.sel.focus.clone();
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    if let Some(para) = session.doc.para_at(&focus) {
                        if focus.off > 0 {
                            let prev_off = motion::word_backward(&para.text, focus.off);
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, prev_off));
                        } else if p_idx > 0 {
                            let prev_p = p_idx - 1;
                            let len = session.doc.para_at(&wordcraft_doc::Pos::body(prev_p, 0)).map(|p| p.len()).unwrap_or(0);
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(prev_p, len));
                        }
                    }
                }
                true
            }
            Some('e') => {
                for _ in 0..cnt {
                    let focus = session.sel.focus.clone();
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    if let Some(para) = session.doc.para_at(&focus) {
                        let next_off = motion::word_end(&para.text, focus.off);
                        if next_off < para.len() || p_idx + 1 >= session.doc.body.len() {
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, next_off));
                        } else {
                            let next_p = p_idx + 1;
                            if let Some(next_para) = session.doc.para_at(&wordcraft_doc::Pos::body(next_p, 0)) {
                                let off = motion::word_end(&next_para.text, 0);
                                session.sel = Selection::caret(wordcraft_doc::Pos::body(next_p, off));
                            }
                        }
                    }
                }
                true
            }
            Some('0') => {
                let focus = session.sel.focus.clone();
                let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, 0));
                true
            }
            Some('$') => {
                let _ = session.run("caret.end", &json!({}));
                true
            }
            Some('^') => {
                let focus = session.sel.focus.clone();
                let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                if let Some(para) = session.doc.para_at(&focus) {
                    let off = motion::first_non_blank(&para.text);
                    session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, off));
                }
                true
            }
            Some('|') => {
                let focus = session.sel.focus.clone();
                let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                if let Some(para) = session.doc.para_at(&focus) {
                    let off = motion::col_offset(&para.text, cnt.max(1));
                    session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, off));
                }
                true
            }
            Some('(') => {
                for _ in 0..cnt {
                    let next_pos = motion::sentence_backward(session, &session.sel.focus);
                    session.sel = Selection::caret(next_pos);
                }
                true
            }
            Some(')') => {
                for _ in 0..cnt {
                    let next_pos = motion::sentence_forward(session, &session.sel.focus);
                    session.sel = Selection::caret(next_pos);
                }
                true
            }
            Some('{') => {
                repeat(cnt, || session.run("caret.paraUp", &json!({})));
                true
            }
            Some('}') => {
                repeat(cnt, || session.run("caret.paraDown", &json!({})));
                true
            }
            Some('[') => {
                self.pending = "[".to_string();
                true
            }
            Some(']') => {
                self.pending = "]".to_string();
                true
            }
            Some('g') => {
                self.pending = "g".to_string();
                true
            }
            Some('G') => {
                if let Some(line_num) = self.count.take() {
                    let para_idx = line_num.saturating_sub(1).min(session.doc.body.len().saturating_sub(1));
                    session.sel = Selection::caret(wordcraft_doc::Pos::body(para_idx, 0));
                } else {
                    let _ = session.run("caret.docEnd", &json!({}));
                }
                true
            }
            Some('z') => {
                self.pending = "z".to_string();
                true
            }
            Some('s') => {
                self.flash_state.start();
                self.pending = "flash".to_string();
                self.status_msg = Some("flash: ".to_string());
                true
            }
            Some('J') => {
                let _ = session.run("caret.end", &json!({}));
                let _ = session.run("text.delete", &json!({}));
                self.last_change = Some(LastVimChange::Join);
                true
            }
            Some('.') => {
                if let Some(ch) = self.last_change.clone() {
                    for _ in 0..cnt {
                        ch.replay(session);
                    }
                }
                true
            }
            Some('f') => {
                self.pending = "f".to_string();
                true
            }
            Some('F') => {
                self.pending = "F".to_string();
                true
            }
            Some('t') => {
                self.pending = "t".to_string();
                true
            }
            Some('T') => {
                self.pending = "T".to_string();
                true
            }
            Some(';') => {
                if let Some(find) = self.last_char_find {
                    motion::execute_char_find(session, find, false);
                }
                true
            }
            Some(',') => {
                if let Some(mut find) = self.last_char_find {
                    find.forward = !find.forward;
                    motion::execute_char_find(session, find, false);
                }
                true
            }
            Some('x') => {
                let reg = self.current_register.take();
                let focus = session.sel.focus.clone();
                if let Some(para) = session.doc.para_at(&focus) {
                    let end_off = (focus.off + cnt).min(para.len());
                    let deleted_str = para.text.get(focus.off..end_off).unwrap_or("").to_string();
                    self.registers.record_delete(reg, deleted_str, RegType::Charwise);
                }
                repeat(cnt, || session.run("text.delete", &json!({})));
                self.last_change = Some(LastVimChange::DeleteChar);
                true
            }
            Some('u') => {
                let _ = session.run("edit.undo", &json!({}));
                true
            }
            Some('p') => {
                let reg = self.current_register.take();
                self.registers.paste(session, reg, false, cnt);
                self.last_change = Some(LastVimChange::Paste);
                true
            }
            Some('P') => {
                let reg = self.current_register.take();
                self.registers.paste(session, reg, true, cnt);
                self.last_change = Some(LastVimChange::Paste);
                true
            }
            Some('i') => {
                self.mode = VimMode::Insert;
                true
            }
            Some('a') => {
                let _ = session.run("caret.right", &json!({}));
                self.mode = VimMode::Insert;
                true
            }
            Some('I') => {
                let focus = session.sel.focus.clone();
                let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                if let Some(para) = session.doc.para_at(&focus) {
                    let off = motion::first_non_blank(&para.text);
                    session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, off));
                }
                self.mode = VimMode::Insert;
                true
            }
            Some('A') => {
                let _ = session.run("caret.end", &json!({}));
                self.mode = VimMode::Insert;
                true
            }
            Some('o') => {
                let _ = session.run("caret.end", &json!({}));
                let _ = session.run("text.newParagraph", &json!({}));
                self.mode = VimMode::Insert;
                true
            }
            Some('O') => {
                let _ = session.run("caret.home", &json!({}));
                let _ = session.run("text.newParagraph", &json!({}));
                let _ = session.run("caret.up", &json!({}));
                self.mode = VimMode::Insert;
                true
            }
            Some('v') => {
                self.mode = VimMode::Visual;
                true
            }
            Some('V') => {
                let _ = session.run("select.line", &json!({}));
                self.mode = VimMode::VisualLine;
                true
            }
            Some(':') => {
                self.mode = VimMode::Command;
                self.cmd_input = ":".to_string();
                true
            }
            Some('/') => {
                self.mode = VimMode::Command;
                self.cmd_input = "/".to_string();
                true
            }
            Some('?') => {
                self.mode = VimMode::Command;
                self.cmd_input = "?".to_string();
                true
            }
            Some('n') => {
                repeat(cnt, || session.run("edit.findNext", &json!({})));
                true
            }
            Some('N') => {
                repeat(cnt, || session.run("edit.findPrevious", &json!({})));
                true
            }
            Some('r') => {
                self.replace_char_pending = true;
                true
            }
            Some('R') => {
                self.mode = VimMode::Replace;
                self.replace_stack.clear();
                self.status_msg = Some("-- REPLACE --".to_string());
                true
            }
            Some('~') => {
                let focus = &session.sel.focus;
                let ch_opt = session.doc.para_at(focus).and_then(|para| para.text.get(focus.off..).and_then(|s| s.chars().next()));
                if let Some(ch) = ch_opt {
                    let toggled = if ch.is_uppercase() { ch.to_lowercase().to_string() } else { ch.to_uppercase().to_string() };
                    let _ = session.run("text.delete", &json!({}));
                    let _ = session.run("text.insert", &json!({"text": toggled}));
                    self.last_change = Some(LastVimChange::ToggleCase);
                }
                true
            }
            Some('C') => {
                let _ = session.run("caret.end", &json!({"extend": true}));
                let _ = session.run("edit.cut", &json!({}));
                self.last_change = Some(LastVimChange::ChangeToEnd { text: String::new() });
                self.mode = VimMode::Insert;
                true
            }
            Some('D') => {
                let _ = session.run("caret.end", &json!({"extend": true}));
                let _ = session.run("edit.cut", &json!({}));
                self.last_change = Some(LastVimChange::DeleteToEnd);
                true
            }
            Some('Y') => {
                let _ = session.run("caret.end", &json!({"extend": true}));
                self.apply_operator_to_selection(session, VimOp::Yank);
                true
            }
            Some('S') => {
                let _ = session.run("select.line", &json!({}));
                let _ = session.run("edit.cut", &json!({}));
                self.last_change = Some(LastVimChange::SubstituteLine { text: String::new() });
                self.mode = VimMode::Insert;
                true
            }
            Some('X') => {
                let _ = session.run("caret.left", &json!({}));
                let _ = session.run("text.delete", &json!({}));
                self.last_change = Some(LastVimChange::DeleteCharBefore);
                true
            }
            Some('*') => {
                let focus = &session.sel.focus;
                if let Some(para) = session.doc.para_at(focus) {
                    let word = extract_word_at(&para.text, focus.off);
                    if !word.is_empty() {
                        let _ = session.run("edit.find", &json!({"text": word}));
                    }
                }
                true
            }
            Some('#') => {
                let focus = &session.sel.focus;
                if let Some(para) = session.doc.para_at(focus) {
                    let word = extract_word_at(&para.text, focus.off);
                    if !word.is_empty() {
                        let _ = session.run("edit.find", &json!({"text": word}));
                        let _ = session.run("edit.findPrevious", &json!({}));
                    }
                }
                true
            }
            Some('%') => {
                let focus = session.sel.focus.clone();
                let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                if let Some(para) = session.doc.para_at(&focus)
                    && let Some(new_off) = motion::find_matching_bracket(&para.text, focus.off)
                {
                    session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, new_off));
                }
                true
            }
            Some('H') => {
                let _ = session.run("caret.docStart", &json!({}));
                true
            }
            Some('M') => {
                let mid = session.doc.body.len() / 2;
                session.sel = Selection::caret(wordcraft_doc::Pos::body(mid, 0));
                true
            }
            Some('L') => {
                let _ = session.run("caret.docEnd", &json!({}));
                true
            }
            Some('W') => {
                for _ in 0..cnt {
                    let focus = session.sel.focus.clone();
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    if let Some(para) = session.doc.para_at(&focus) {
                        let next_off = motion::big_word_forward(&para.text, focus.off);
                        if next_off < para.len() || p_idx + 1 >= session.doc.body.len() {
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, next_off));
                        } else {
                            let next_p = p_idx + 1;
                            let off =
                                session.doc.para_at(&wordcraft_doc::Pos::body(next_p, 0)).map(|p| motion::first_non_blank(&p.text)).unwrap_or(0);
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(next_p, off));
                        }
                    }
                }
                true
            }
            Some('B') => {
                for _ in 0..cnt {
                    let focus = session.sel.focus.clone();
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    if let Some(para) = session.doc.para_at(&focus) {
                        if focus.off > 0 {
                            let prev_off = motion::big_word_backward(&para.text, focus.off);
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, prev_off));
                        } else if p_idx > 0 {
                            let prev_p = p_idx - 1;
                            let len = session.doc.para_at(&wordcraft_doc::Pos::body(prev_p, 0)).map(|p| p.len()).unwrap_or(0);
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(prev_p, len));
                        }
                    }
                }
                true
            }
            Some('E') => {
                for _ in 0..cnt {
                    let focus = session.sel.focus.clone();
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    if let Some(para) = session.doc.para_at(&focus) {
                        let next_off = motion::big_word_end(&para.text, focus.off);
                        if next_off < para.len() || p_idx + 1 >= session.doc.body.len() {
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, next_off));
                        } else {
                            let next_p = p_idx + 1;
                            if let Some(next_para) = session.doc.para_at(&wordcraft_doc::Pos::body(next_p, 0)) {
                                let off = motion::big_word_end(&next_para.text, 0);
                                session.sel = Selection::caret(wordcraft_doc::Pos::body(next_p, off));
                            }
                        }
                    }
                }
                true
            }
            Some('m') => {
                self.pending = "m".to_string();
                true
            }
            Some('\'') => {
                self.pending = "'".to_string();
                true
            }
            Some('`') => {
                self.pending = "`".to_string();
                true
            }
            Some('"') => {
                self.pending = "\"".to_string();
                true
            }
            Some('q') => {
                if self.macro_state.recording.is_some() {
                    let reg = self.macro_state.stop_recording().unwrap_or(' ');
                    self.status_msg = Some(format!("macro recorded to @{reg}"));
                } else {
                    self.pending = "q".to_string();
                }
                true
            }
            Some('@') => {
                self.pending = "@".to_string();
                true
            }
            _ => match key_name {
                "Left" => {
                    repeat(cnt, || session.run("caret.left", &json!({})));
                    true
                }
                "Right" => {
                    repeat(cnt, || session.run("caret.right", &json!({})));
                    true
                }
                "Up" => {
                    repeat(cnt, || session.run("caret.up", &json!({})));
                    true
                }
                "Down" => {
                    repeat(cnt, || session.run("caret.down", &json!({})));
                    true
                }
                "Home" => {
                    let _ = session.run("caret.home", &json!({}));
                    true
                }
                "End" => {
                    let _ = session.run("caret.end", &json!({}));
                    true
                }
                _ => true,
            },
        }
    }

    fn handle_operator_pending(&mut self, session: &mut Session, op: VimOp, key_name: &str, input_char: Option<char>) -> bool {
        // Double operator: dd, cc, yy, >>, <<, ==, g~~, guu, gUU, gqq (applies to line)
        let is_double = matches!(
            (op, input_char),
            (VimOp::Delete, Some('d'))
                | (VimOp::Change, Some('c'))
                | (VimOp::Yank, Some('y'))
                | (VimOp::ShiftRight, Some('>'))
                | (VimOp::ShiftLeft, Some('<'))
                | (VimOp::Indent, Some('='))
                | (VimOp::ToggleCase, Some('~') | Some('g'))
                | (VimOp::LowerCase, Some('u') | Some('g'))
                | (VimOp::UpperCase, Some('U') | Some('g'))
                | (VimOp::Format, Some('q') | Some('w') | Some('g'))
        );

        if is_double {
            let cnt = self.op_count.take().unwrap_or(1).saturating_mul(self.take_count());
            for _ in 0..cnt {
                let _ = session.run("select.line", &json!({}));
                self.apply_operator_internal(session, op, true);
                match op {
                    VimOp::Delete => self.last_change = Some(LastVimChange::DeleteLine),
                    VimOp::ShiftRight => self.last_change = Some(LastVimChange::Indent),
                    VimOp::ShiftLeft => self.last_change = Some(LastVimChange::Outdent),
                    _ => {}
                }
            }
            self.reset_pending();
            return true;
        }

        // Inline character search following operator: df., dt), dF:, dT;
        if self.pending.is_empty()
            && let Some(c) = input_char
            && matches!(c, 'f' | 'F' | 't' | 'T')
        {
            self.pending = c.to_string();
            return true;
        }

        if !self.pending.is_empty() && matches!(self.pending.as_str(), "f" | "F" | "t" | "T") {
            let p = self.pending.clone();
            if let Some(target) = input_char {
                let forward = p == "f" || p == "t";
                let till = p == "t" || p == "T";
                let find = motion::CharFind { target, forward, till };
                self.last_char_find = Some(find);
                if motion::execute_char_find(session, find, true) {
                    self.apply_operator_to_selection(session, op);
                    if op == VimOp::Delete {
                        self.last_change = Some(LastVimChange::DeleteCharFind(find));
                    }
                }
            }
            self.reset_pending();
            return true;
        }

        // Delete surround: ds{char}
        if op == VimOp::Delete && self.pending.is_empty() && input_char == Some('s') {
            self.pending = "ds".to_string();
            return true;
        }

        if op == VimOp::Delete && self.pending == "ds" {
            if let Some(target) = input_char
                && surround::delete_surround(session, target)
            {
                self.last_change = Some(LastVimChange::SurroundDelete(target));
            }
            self.reset_pending();
            return true;
        }

        // Change surround: cs{old}{new}
        if op == VimOp::Change && self.pending.is_empty() && input_char == Some('s') {
            self.pending = "cs".to_string();
            return true;
        }

        if op == VimOp::Change && self.pending.starts_with("cs") {
            if self.pending == "cs" {
                if let Some(old) = input_char {
                    self.pending = format!("cs{old}");
                    return true;
                }
            } else if self.pending.len() == 3 {
                if let Some(new) = input_char {
                    let old = self.pending.chars().nth(2).unwrap_or('"');
                    if surround::change_surround(session, old, new) {
                        self.last_change = Some(LastVimChange::SurroundChange { old, new });
                    }
                }
                self.reset_pending();
                return true;
            }
        }

        // Surround operator: ys{motion/obj}{char}
        if op == VimOp::Yank && self.pending.is_empty() && input_char == Some('s') {
            self.pending = "ys".to_string();
            return true;
        }

        if op == VimOp::Yank && self.pending.starts_with("ys") {
            if self.pending == "ys" {
                if let Some(c) = input_char {
                    if c == 'i' || c == 'a' {
                        self.pending = format!("ys{c}");
                        return true;
                    } else if c == 'w' {
                        self.pending = "ysw".to_string();
                        return true;
                    }
                }
            } else if self.pending == "ysi" || self.pending == "ysa" {
                if let Some(target) = input_char {
                    self.pending = format!("{}{target}", self.pending);
                    return true;
                }
            } else if self.pending.len() == 4 {
                if let Some(delim) = input_char {
                    let inner = self.pending.chars().nth(2) == Some('i');
                    let target = self.pending.chars().nth(3).unwrap_or('w');
                    if let Some((start, end)) = text_object::find_text_object(session, target, inner) {
                        session.sel = Selection { anchor: start, focus: end };
                        surround::wrap_selection(session, delim);
                        self.last_change = Some(LastVimChange::SurroundWrap { target, inner, delim });
                    }
                }
                self.reset_pending();
                return true;
            } else if self.pending == "ysw" {
                if let Some(delim) = input_char {
                    let _ = session.run("select.word", &json!({}));
                    surround::wrap_selection(session, delim);
                }
                self.reset_pending();
                return true;
            }
        }

        // Text object prefix: i (inner) or a (around)
        if self.pending.is_empty()
            && let Some(c) = input_char
            && (c == 'i' || c == 'a')
        {
            self.pending = c.to_string();
            return true;
        }

        // Completing text object: e.g. iw, aw, i", a", i(, a(
        if !self.pending.is_empty() && (self.pending == "i" || self.pending == "a") {
            let inner = self.pending == "i";
            if let Some(target) = input_char
                && let Some((start, end)) = text_object::find_text_object(session, target, inner)
            {
                session.sel = Selection { anchor: start, focus: end };
                self.apply_operator_to_selection(session, op);
                if op == VimOp::Delete {
                    self.last_change = Some(LastVimChange::DeleteTextObject { target, inner });
                }
            }
            self.reset_pending();
            return true;
        }

        // Motion following operator (dw, d$, d0, cw, etc.)
        let ext = json!({"extend": true});
        let cnt = self.op_count.take().unwrap_or(1).saturating_mul(self.take_count());
        let ran = match input_char {
            Some('w') => {
                for _ in 0..cnt {
                    let focus = session.sel.focus.clone();
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    if let Some(para) = session.doc.para_at(&focus) {
                        let next_off = motion::word_forward(&para.text, focus.off);
                        if next_off < para.len() || p_idx + 1 >= session.doc.body.len() {
                            session.sel = Selection { anchor: session.sel.anchor.clone(), focus: wordcraft_doc::Pos::body(p_idx, next_off) };
                        } else {
                            let next_p = p_idx + 1;
                            let off =
                                session.doc.para_at(&wordcraft_doc::Pos::body(next_p, 0)).map(|p| motion::first_non_blank(&p.text)).unwrap_or(0);
                            session.sel = Selection { anchor: session.sel.anchor.clone(), focus: wordcraft_doc::Pos::body(next_p, off) };
                        }
                    }
                }
                true
            }
            Some('e') => {
                for _ in 0..cnt {
                    let focus = session.sel.focus.clone();
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    if let Some(para) = session.doc.para_at(&focus) {
                        let next_off = motion::word_end(&para.text, focus.off);
                        let end_off = (next_off + 1).min(para.len());
                        session.sel = Selection { anchor: session.sel.anchor.clone(), focus: wordcraft_doc::Pos::body(p_idx, end_off) };
                    }
                }
                true
            }
            Some('b') => {
                for _ in 0..cnt {
                    let focus = session.sel.focus.clone();
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    if let Some(para) = session.doc.para_at(&focus) {
                        let prev_off = motion::word_backward(&para.text, focus.off);
                        session.sel = Selection { anchor: session.sel.anchor.clone(), focus: wordcraft_doc::Pos::body(p_idx, prev_off) };
                    }
                }
                true
            }
            Some('W') => {
                for _ in 0..cnt {
                    let focus = session.sel.focus.clone();
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    if let Some(para) = session.doc.para_at(&focus) {
                        let next_off = motion::big_word_forward(&para.text, focus.off);
                        if next_off < para.len() || p_idx + 1 >= session.doc.body.len() {
                            session.sel = Selection { anchor: session.sel.anchor.clone(), focus: wordcraft_doc::Pos::body(p_idx, next_off) };
                        } else {
                            let next_p = p_idx + 1;
                            let off =
                                session.doc.para_at(&wordcraft_doc::Pos::body(next_p, 0)).map(|p| motion::first_non_blank(&p.text)).unwrap_or(0);
                            session.sel = Selection { anchor: session.sel.anchor.clone(), focus: wordcraft_doc::Pos::body(next_p, off) };
                        }
                    }
                }
                true
            }
            Some('E') => {
                for _ in 0..cnt {
                    let focus = session.sel.focus.clone();
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    if let Some(para) = session.doc.para_at(&focus) {
                        let next_off = motion::big_word_end(&para.text, focus.off);
                        let end_off = (next_off + 1).min(para.len());
                        session.sel = Selection { anchor: session.sel.anchor.clone(), focus: wordcraft_doc::Pos::body(p_idx, end_off) };
                    }
                }
                true
            }
            Some('B') => {
                for _ in 0..cnt {
                    let focus = session.sel.focus.clone();
                    let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                    if let Some(para) = session.doc.para_at(&focus) {
                        let prev_off = motion::big_word_backward(&para.text, focus.off);
                        session.sel = Selection { anchor: session.sel.anchor.clone(), focus: wordcraft_doc::Pos::body(p_idx, prev_off) };
                    }
                }
                true
            }
            Some('$') => {
                let _ = session.run("caret.end", &ext);
                true
            }
            Some('0') => {
                let focus = session.sel.focus.clone();
                let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                session.sel = Selection { anchor: session.sel.anchor.clone(), focus: wordcraft_doc::Pos::body(p_idx, 0) };
                true
            }
            Some('^') => {
                let focus = session.sel.focus.clone();
                let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                if let Some(para) = session.doc.para_at(&focus) {
                    let off = motion::first_non_blank(&para.text);
                    session.sel = Selection { anchor: session.sel.anchor.clone(), focus: wordcraft_doc::Pos::body(p_idx, off) };
                }
                true
            }
            Some('(') => {
                for _ in 0..cnt {
                    let new_pos = motion::sentence_backward(session, &session.sel.focus);
                    session.sel = Selection { anchor: session.sel.anchor.clone(), focus: new_pos };
                }
                true
            }
            Some(')') => {
                for _ in 0..cnt {
                    let new_pos = motion::sentence_forward(session, &session.sel.focus);
                    session.sel = Selection { anchor: session.sel.anchor.clone(), focus: new_pos };
                }
                true
            }
            Some('{') => {
                repeat(cnt, || session.run("caret.paraUp", &ext));
                true
            }
            Some('}') => {
                repeat(cnt, || session.run("caret.paraDown", &ext));
                true
            }
            Some('G') => {
                let _ = session.run("caret.docEnd", &ext);
                true
            }
            Some('%') => {
                let focus = session.sel.focus.clone();
                let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
                if let Some(para) = session.doc.para_at(&focus)
                    && let Some(target) = motion::find_matching_bracket(&para.text, focus.off)
                {
                    session.sel =
                        Selection { anchor: session.sel.anchor.clone(), focus: wordcraft_doc::Pos::body(p_idx, (target + 1).min(para.len())) };
                    true
                } else {
                    false
                }
            }
            _ => match key_name {
                "Left" => {
                    repeat(cnt, || session.run("caret.left", &ext));
                    true
                }
                "Right" => {
                    repeat(cnt, || session.run("caret.right", &ext));
                    true
                }
                "Up" => {
                    repeat(cnt, || session.run("caret.up", &ext));
                    true
                }
                "Down" => {
                    repeat(cnt, || session.run("caret.down", &ext));
                    true
                }
                _ => false,
            },
        };

        if ran {
            self.apply_operator_to_selection(session, op);
            if op == VimOp::Delete
                && let Some(m) = input_char
            {
                let m_name = match m {
                    'w' => "caret.wordRight",
                    'b' => "caret.wordLeft",
                    'e' => "caret.wordRight",
                    '$' => "caret.end",
                    '0' | '^' => "caret.home",
                    '{' => "caret.paraUp",
                    '}' => "caret.paraDown",
                    'G' => "caret.docEnd",
                    _ => "caret.wordRight",
                };
                self.last_change = Some(LastVimChange::DeleteMotion(m_name.to_string()));
            }
            self.reset_pending();
            return true;
        }

        self.reset_pending();
        true
    }

    fn apply_operator_to_selection(&mut self, session: &mut Session, op: VimOp) {
        self.apply_operator_internal(session, op, false);
    }

    fn apply_operator_internal(&mut self, session: &mut Session, op: VimOp, force_linewise: bool) {
        let reg = self.current_register.take();
        let selected_text = session.selected_text();
        let is_linewise = force_linewise || self.mode == VimMode::VisualLine || selected_text.ends_with('\n');
        let reg_type = if is_linewise { RegType::Linewise } else { RegType::Charwise };

        if reg == Some('_') {
            // Black hole register: don't copy to clipboard or registers
            if op == VimOp::Delete || op == VimOp::Change {
                let _ = session.run("text.delete", &json!({}));
                if op == VimOp::Change {
                    self.mode = VimMode::Insert;
                }
            } else {
                let _ = session.run("select.collapse", &json!({}));
            }
            return;
        }

        match op {
            VimOp::Delete => {
                self.registers.record_delete(reg, selected_text, reg_type);
                let _ = session.run("edit.cut", &json!({}));
            }
            VimOp::Change => {
                self.registers.record_delete(reg, selected_text, reg_type);
                let _ = session.run("edit.cut", &json!({}));
                self.mode = VimMode::Insert;
            }
            VimOp::Yank => {
                self.registers.record_yank(reg, selected_text, reg_type);
                let _ = session.run("edit.copy", &json!({}));
                let _ = session.run("select.collapse", &json!({}));
            }
            VimOp::ShiftRight => {
                let _ = session.run("para.indent", &json!({}));
                let _ = session.run("select.collapse", &json!({}));
            }
            VimOp::ShiftLeft => {
                let _ = session.run("para.outdent", &json!({}));
                let _ = session.run("select.collapse", &json!({}));
            }
            VimOp::Indent => {
                let _ = session.run("select.collapse", &json!({}));
            }
            VimOp::ToggleCase => {
                apply_case_to_selection(session, CaseOp::Toggle);
            }
            VimOp::LowerCase => {
                apply_case_to_selection(session, CaseOp::Lower);
            }
            VimOp::UpperCase => {
                apply_case_to_selection(session, CaseOp::Upper);
            }
            VimOp::Format => {
                let _ = session.run("select.collapse", &json!({}));
            }
        }
    }

    fn handle_visual(&mut self, session: &mut Session, key_name: &str, text: Option<&str>, ctrl: bool, _shift: bool) -> bool {
        if key_name == "Escape" {
            self.last_visual_sel = Some(session.sel.clone());
            let _ = session.run("select.collapse", &json!({}));
            session.column = None;
            self.mode = VimMode::Normal;
            self.reset_pending();
            return true;
        }

        // Ctrl+V toggles Visual Block mode
        if ctrl && key_name == "V" {
            if self.mode == VimMode::VisualBlock {
                let _ = session.run("select.collapse", &json!({}));
                session.column = None;
                self.mode = VimMode::Normal;
            } else {
                self.mode = VimMode::VisualBlock;
                let _ = session.run("select.column", &json!({"anchor": session.sel.anchor, "focus": session.sel.focus}));
            }
            return true;
        }

        let input_char = text.and_then(|t| t.chars().next());
        let ext = json!({"extend": true});

        // ':' in visual modes pre-fills :'<,'>
        if input_char == Some(':') {
            self.mode = VimMode::Command;
            self.cmd_input = ":'<,'>".to_string();
            return true;
        }

        // Visual mode toggles
        if input_char == Some('v') {
            if self.mode == VimMode::Visual {
                let _ = session.run("select.collapse", &json!({}));
                self.mode = VimMode::Normal;
            } else {
                self.mode = VimMode::Visual;
                session.column = None;
            }
            return true;
        }

        if input_char == Some('V') {
            if self.mode == VimMode::VisualLine {
                let _ = session.run("select.collapse", &json!({}));
                self.mode = VimMode::Normal;
            } else {
                let _ = session.run("select.line", &json!({}));
                self.mode = VimMode::VisualLine;
                session.column = None;
            }
            return true;
        }

        // Visual Block specific operations: I, A, c, d, x, y
        if self.mode == VimMode::VisualBlock {
            match input_char {
                Some('I') => {
                    if let Some(segs) = session.column_segments().map(<[(wordcraft_doc::Pos, wordcraft_doc::Pos)]>::to_vec) {
                        let lines: Vec<usize> = segs.iter().map(|(a, _)| a.path.0.first().copied().unwrap_or(0) as usize).collect();
                        let min_col = segs.iter().map(|(a, b)| a.off.min(b.off)).min().unwrap_or(0);
                        if let Some(&first_line) = lines.first() {
                            self.block_edit = Some(BlockEditState { lines: lines.into_iter().skip(1).collect(), col: min_col, is_append: false });
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(first_line, min_col));
                            session.column = None;
                            self.mode = VimMode::Insert;
                            self.insert_buf.clear();
                            return true;
                        }
                    }
                }
                Some('A') => {
                    if let Some(segs) = session.column_segments().map(<[(wordcraft_doc::Pos, wordcraft_doc::Pos)]>::to_vec) {
                        let lines: Vec<usize> = segs.iter().map(|(a, _)| a.path.0.first().copied().unwrap_or(0) as usize).collect();
                        let max_col = segs.iter().map(|(a, b)| a.off.max(b.off)).max().unwrap_or(0);
                        if let Some(&first_line) = lines.first() {
                            self.block_edit = Some(BlockEditState { lines: lines.into_iter().skip(1).collect(), col: max_col, is_append: true });
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(first_line, max_col));
                            session.column = None;
                            self.mode = VimMode::Insert;
                            self.insert_buf.clear();
                            return true;
                        }
                    }
                }
                Some('c') => {
                    if let Some(segs) = session.column_segments().map(<[(wordcraft_doc::Pos, wordcraft_doc::Pos)]>::to_vec) {
                        let lines: Vec<usize> = segs.iter().map(|(a, _)| a.path.0.first().copied().unwrap_or(0) as usize).collect();
                        let min_col = segs.iter().map(|(a, b)| a.off.min(b.off)).min().unwrap_or(0);
                        let _ = session.run("edit.cut", &json!({}));
                        if let Some(&first_line) = lines.first() {
                            self.block_edit = Some(BlockEditState { lines: lines.into_iter().skip(1).collect(), col: min_col, is_append: false });
                            session.sel = Selection::caret(wordcraft_doc::Pos::body(first_line, min_col));
                            session.column = None;
                            self.mode = VimMode::Insert;
                            self.insert_buf.clear();
                            return true;
                        }
                    }
                }
                Some('d') | Some('x') => {
                    let _ = session.run("edit.cut", &json!({}));
                    session.column = None;
                    self.mode = VimMode::Normal;
                    return true;
                }
                Some('y') => {
                    let _ = session.run("edit.copy", &json!({}));
                    let _ = session.run("select.collapse", &json!({}));
                    session.column = None;
                    self.mode = VimMode::Normal;
                    return true;
                }
                _ => {}
            }
        }

        // Visual leader mappings
        if !self.pending.is_empty() && self.pending.starts_with("<leader>") {
            if let Some(c) = input_char {
                let mut full = self.pending.clone();
                full.push(c);
                if let Some(target) = self.config.visual_mappings.get(&full).cloned() {
                    self.execute_mapping(session, &target);
                    self.mode = VimMode::Normal;
                    self.reset_pending();
                    return true;
                }
            }
            self.reset_pending();
            return true;
        }

        if self.pending.is_empty()
            && let Some(c) = input_char
            && self.config.leader == c.to_string()
        {
            self.pending = "<leader>".to_string();
            return true;
        }

        // Visual inline char search: vf., vt)
        if self.pending.is_empty()
            && let Some(c) = input_char
            && matches!(c, 'f' | 'F' | 't' | 'T')
        {
            self.pending = c.to_string();
            return true;
        }

        if !self.pending.is_empty() && matches!(self.pending.as_str(), "f" | "F" | "t" | "T") {
            let p = self.pending.clone();
            if let Some(target) = input_char {
                let forward = p == "f" || p == "t";
                let till = p == "t" || p == "T";
                let find = motion::CharFind { target, forward, till };
                self.last_char_find = Some(find);
                motion::execute_char_find(session, find, true);
            }
            self.reset_pending();
            return true;
        }

        // Surround in visual mode: S{char}
        if self.pending.is_empty() && input_char == Some('S') {
            self.pending = "S".to_string();
            return true;
        }

        if self.pending == "S"
            && let Some(delimiter) = input_char
        {
            surround::wrap_selection(session, delimiter);
            self.mode = VimMode::Normal;
            self.reset_pending();
            return true;
        }

        // Text object selection while in Visual mode (e.g. `viw`, `va"`)
        if self.pending.is_empty()
            && let Some(c) = input_char
            && (c == 'i' || c == 'a')
        {
            self.pending = c.to_string();
            return true;
        }

        if !self.pending.is_empty() && (self.pending == "i" || self.pending == "a") {
            let inner = self.pending == "i";
            if let Some(target) = input_char
                && let Some((start, end)) = text_object::find_text_object(session, target, inner)
            {
                session.sel = Selection { anchor: start, focus: end };
            }
            self.pending.clear();
            return true;
        }

        let handled = match input_char {
            Some('h') => {
                let _ = session.run("caret.left", &ext);
                true
            }
            Some('l') => {
                let _ = session.run("caret.right", &ext);
                true
            }
            Some('j') => {
                let _ = session.run("caret.down", &ext);
                true
            }
            Some('k') => {
                let _ = session.run("caret.up", &ext);
                true
            }
            Some('w') => {
                let _ = session.run("caret.wordRight", &ext);
                true
            }
            Some('b') => {
                let _ = session.run("caret.wordLeft", &ext);
                true
            }
            Some('e') => {
                let _ = session.run("caret.wordRight", &ext);
                true
            }
            Some('0') => {
                let _ = session.run("caret.home", &ext);
                true
            }
            Some('$') => {
                let _ = session.run("caret.end", &ext);
                true
            }
            Some('{') => {
                let _ = session.run("caret.paraUp", &ext);
                true
            }
            Some('}') => {
                let _ = session.run("caret.paraDown", &ext);
                true
            }
            Some('G') => {
                let _ = session.run("caret.docEnd", &ext);
                true
            }
            Some('J') => {
                let (start, _) = session.sel.ordered();
                let p_idx = start.path.0.first().copied().unwrap_or(0) as usize;
                if p_idx.saturating_add(1) < session.doc.body.len() {
                    session.doc.body.swap(p_idx, p_idx.saturating_add(1));
                    let new_p = p_idx.saturating_add(1);
                    let len = session.doc.para_at(&wordcraft_doc::Pos::body(new_p, 0)).map(|p| p.len()).unwrap_or(0);
                    session.sel = Selection { anchor: wordcraft_doc::Pos::body(new_p, 0), focus: wordcraft_doc::Pos::body(new_p, len) };
                    session.relayout();
                }
                true
            }
            Some('K') => {
                let (start, _) = session.sel.ordered();
                let p_idx = start.path.0.first().copied().unwrap_or(0) as usize;
                if p_idx > 0 {
                    session.doc.body.swap(p_idx, p_idx.saturating_sub(1));
                    let new_p = p_idx.saturating_sub(1);
                    let len = session.doc.para_at(&wordcraft_doc::Pos::body(new_p, 0)).map(|p| p.len()).unwrap_or(0);
                    session.sel = Selection { anchor: wordcraft_doc::Pos::body(new_p, 0), focus: wordcraft_doc::Pos::body(new_p, len) };
                    session.relayout();
                }
                true
            }
            Some(';') => {
                if let Some(find) = self.last_char_find {
                    motion::execute_char_find(session, find, true);
                }
                true
            }
            Some(',') => {
                if let Some(mut find) = self.last_char_find {
                    find.forward = !find.forward;
                    motion::execute_char_find(session, find, true);
                }
                true
            }
            Some('o') | Some('O') if self.mode != VimMode::VisualBlock => {
                session.sel = Selection { anchor: session.sel.focus.clone(), focus: session.sel.anchor.clone() };
                true
            }
            Some('~') => {
                let (start, end) = session.sel.ordered();
                let start_p = start.path.0.first().copied().unwrap_or(0) as usize;
                let end_p = end.path.0.first().copied().unwrap_or(0) as usize;
                for p_idx in start_p..=end_p {
                    let pos = wordcraft_doc::Pos::body(p_idx, 0);
                    if let Some(para) = session.doc.para_at(&pos).cloned() {
                        let lo = if p_idx == start_p { start.off.min(para.len()) } else { 0 };
                        let hi = if p_idx == end_p { end.off.min(para.len()) } else { para.len() };
                        if lo < hi
                            && let Some(slice) = para.text.get(lo..hi)
                        {
                            let toggled: String = slice
                                .chars()
                                .map(|c| if c.is_uppercase() { c.to_lowercase().next().unwrap_or(c) } else { c.to_uppercase().next().unwrap_or(c) })
                                .collect();
                            let pa = wordcraft_doc::Pos::body(p_idx, lo);
                            let pb = wordcraft_doc::Pos::body(p_idx, hi);
                            let props = para.props_of_char(lo).clone();
                            if let Ok(at) = session.doc.delete_range(&pa, &pb) {
                                let _ = session.doc.insert_text(&at, &toggled, &props);
                            }
                        }
                    }
                }
                session.relayout();
                let _ = session.run("select.collapse", &json!({}));
                self.mode = VimMode::Normal;
                true
            }
            Some('u') if self.mode == VimMode::Visual || self.mode == VimMode::VisualLine => {
                let (start, end) = session.sel.ordered();
                let start_p = start.path.0.first().copied().unwrap_or(0) as usize;
                let end_p = end.path.0.first().copied().unwrap_or(0) as usize;
                for p_idx in start_p..=end_p {
                    let pos = wordcraft_doc::Pos::body(p_idx, 0);
                    if let Some(para) = session.doc.para_at(&pos).cloned() {
                        let lo = if p_idx == start_p { start.off.min(para.len()) } else { 0 };
                        let hi = if p_idx == end_p { end.off.min(para.len()) } else { para.len() };
                        if lo < hi
                            && let Some(slice) = para.text.get(lo..hi)
                        {
                            let lower = slice.to_lowercase();
                            let pa = wordcraft_doc::Pos::body(p_idx, lo);
                            let pb = wordcraft_doc::Pos::body(p_idx, hi);
                            let props = para.props_of_char(lo).clone();
                            if let Ok(at) = session.doc.delete_range(&pa, &pb) {
                                let _ = session.doc.insert_text(&at, &lower, &props);
                            }
                        }
                    }
                }
                session.relayout();
                let _ = session.run("select.collapse", &json!({}));
                self.mode = VimMode::Normal;
                true
            }
            Some('U') if self.mode == VimMode::Visual || self.mode == VimMode::VisualLine => {
                let (start, end) = session.sel.ordered();
                let start_p = start.path.0.first().copied().unwrap_or(0) as usize;
                let end_p = end.path.0.first().copied().unwrap_or(0) as usize;
                for p_idx in start_p..=end_p {
                    let pos = wordcraft_doc::Pos::body(p_idx, 0);
                    if let Some(para) = session.doc.para_at(&pos).cloned() {
                        let lo = if p_idx == start_p { start.off.min(para.len()) } else { 0 };
                        let hi = if p_idx == end_p { end.off.min(para.len()) } else { para.len() };
                        if lo < hi
                            && let Some(slice) = para.text.get(lo..hi)
                        {
                            let upper = slice.to_uppercase();
                            let pa = wordcraft_doc::Pos::body(p_idx, lo);
                            let pb = wordcraft_doc::Pos::body(p_idx, hi);
                            let props = para.props_of_char(lo).clone();
                            if let Ok(at) = session.doc.delete_range(&pa, &pb) {
                                let _ = session.doc.insert_text(&at, &upper, &props);
                            }
                        }
                    }
                }
                session.relayout();
                let _ = session.run("select.collapse", &json!({}));
                self.mode = VimMode::Normal;
                true
            }
            Some('y') => {
                self.last_visual_sel = Some(session.sel.clone());
                self.apply_operator_to_selection(session, VimOp::Yank);
                self.mode = VimMode::Normal;
                true
            }
            Some('d') | Some('x') => {
                self.last_visual_sel = Some(session.sel.clone());
                self.apply_operator_to_selection(session, VimOp::Delete);
                self.mode = VimMode::Normal;
                true
            }
            Some('c') => {
                self.last_visual_sel = Some(session.sel.clone());
                self.apply_operator_to_selection(session, VimOp::Change);
                true
            }
            Some('p') => {
                let reg = self.current_register.take();
                let _ = session.run("text.delete", &json!({}));
                self.registers.paste(session, reg, true, 1);
                self.mode = VimMode::Normal;
                true
            }
            Some('>') => {
                let _ = session.run("para.indent", &json!({}));
                true
            }
            Some('<') => {
                let _ = session.run("para.outdent", &json!({}));
                true
            }
            _ => match key_name {
                "Left" => {
                    let _ = session.run("caret.left", &ext);
                    true
                }
                "Right" => {
                    let _ = session.run("caret.right", &ext);
                    true
                }
                "Up" => {
                    let _ = session.run("caret.up", &ext);
                    true
                }
                "Down" => {
                    let _ = session.run("caret.down", &ext);
                    true
                }
                _ => true,
            },
        };

        if handled && self.mode == VimMode::VisualBlock {
            let _ = session.run("select.column", &json!({"anchor": session.sel.anchor, "focus": session.sel.focus}));
        }
        handled
    }

    fn execute_mapping(&mut self, session: &mut Session, target: &str) {
        if target.starts_with(':') {
            let res = ex::execute_ex(session, target);
            if !res.message.is_empty() {
                self.status_msg = Some(res.message);
            }
        } else {
            let _ = session.run(target, &json!({}));
        }
    }

    fn handle_command(&mut self, session: &mut Session, key_name: &str, text: Option<&str>) -> bool {
        if key_name == "Escape" {
            self.cmd_input.clear();
            self.mode = VimMode::Normal;
            self.reset_pending();
            return true;
        }

        if key_name == "Backspace" {
            self.cmd_input.pop();
            if self.cmd_input.is_empty() {
                self.mode = VimMode::Normal;
                self.reset_pending();
            }
            return true;
        }

        if key_name == "Enter" || key_name == "Return" {
            let input = std::mem::take(&mut self.cmd_input);
            if input.starts_with('/') || input.starts_with('?') {
                let is_rev = input.starts_with('?');
                let query = input.get(1..).unwrap_or("").trim();
                if !query.is_empty() {
                    let _ = session.run("edit.find", &json!({"text": query}));
                    if is_rev {
                        let _ = session.run("edit.findPrevious", &json!({}));
                    }
                    self.status_msg = Some(input);
                }
            } else if input.starts_with(':') {
                let res = ex::execute_ex(session, &input);
                if !res.message.is_empty() {
                    self.status_msg = Some(res.message);
                }
            }
            self.mode = VimMode::Normal;
            self.reset_pending();
            return true;
        }

        if let Some(t) = text {
            self.cmd_input.push_str(t);
            return true;
        }

        true
    }

    /// Increment or decrement the number under or after the cursor (<C-a>/<C-x>).
    pub fn handle_arithmetic(&mut self, session: &mut Session, delta: isize) -> bool {
        let focus = session.sel.focus.clone();
        let Some(para) = session.doc.para_at(&focus) else { return false };
        let text = para.text.clone();
        let off = focus.off.min(para.len());
        let bytes = text.as_bytes();

        let mut idx = off;
        while idx > 0 && bytes.get(idx.saturating_sub(1)).is_some_and(|b| b.is_ascii_digit() || *b == b'-') {
            idx -= 1;
        }

        let mut num_start = None;
        let mut num_end = None;
        while idx < bytes.len() {
            if bytes[idx].is_ascii_digit() || (bytes[idx] == b'-' && bytes.get(idx + 1).is_some_and(|b| b.is_ascii_digit())) {
                let start = idx;
                idx += 1;
                while idx < bytes.len() && bytes[idx].is_ascii_digit() {
                    idx += 1;
                }
                num_start = Some(start);
                num_end = Some(idx);
                break;
            }
            idx += 1;
        }

        let Some((start, end)) = num_start.zip(num_end) else {
            return false;
        };
        let Some(num_str) = text.get(start..end) else {
            return false;
        };
        let Ok(val) = num_str.parse::<isize>() else {
            return false;
        };

        let count = self.take_count() as isize;
        let new_val = val.saturating_add(delta.saturating_mul(count));
        let new_str = new_val.to_string();
        let p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
        let pa = wordcraft_doc::Pos::body(p_idx, start);
        let pb = wordcraft_doc::Pos::body(p_idx, end);
        let props = para.props_of_char(start).clone();
        if let Ok(at) = session.doc.delete_range(&pa, &pb) {
            let _ = session.doc.insert_text(&at, &new_str, &props);
            session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, start + new_str.len().saturating_sub(1)));
            session.relayout();
            self.change_list.push(session.sel.focus.clone());
            return true;
        }
        false
    }
}

/// Extract the word under or nearest to the given byte offset.
fn extract_word_at(text: &str, off: usize) -> String {
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
            break;
        }
        end = off + i + c.len_utf8();
    }
    text.get(start..end).unwrap_or("").to_string()
}

/// Align equal signs across selected paragraphs or the paragraph block around the cursor.
fn align_equal_signs(session: &mut Session) {
    let (sel_start, sel_end) = session.sel.ordered();
    let start_p = sel_start.path.0.first().copied().unwrap_or(0) as usize;
    let end_p = sel_end.path.0.first().copied().unwrap_or(0) as usize;

    let range = if start_p == end_p {
        let mut lo = start_p;
        while lo > 0 && session.doc.body.get(lo - 1).and_then(|p| p.as_para()).map(|p| p.text.contains('=')).unwrap_or(false) {
            lo -= 1;
        }
        let mut hi = end_p;
        while hi + 1 < session.doc.body.len() && session.doc.body.get(hi + 1).and_then(|p| p.as_para()).map(|p| p.text.contains('=')).unwrap_or(false)
        {
            hi += 1;
        }
        lo..=hi
    } else {
        start_p..=end_p
    };

    let mut max_eq_col = 0;
    for p_idx in range.clone() {
        let eq_opt = session.doc.body.get(p_idx).and_then(|p| p.as_para()).and_then(|para| para.text.find('=').map(|idx| (para, idx)));
        if let Some((para, idx)) = eq_opt {
            let left_len = para.text.get(..idx).unwrap_or("").trim_end().chars().count();
            max_eq_col = max_eq_col.max(left_len);
        }
    }

    if max_eq_col == 0 {
        return;
    }

    for p_idx in range {
        let pos = wordcraft_doc::Pos::body(p_idx, 0);
        let Some(para) = session.doc.para_at(&pos).cloned() else { continue };
        if let Some(idx) = para.text.find('=') {
            let left = para.text.get(..idx).unwrap_or("").trim_end();
            let right = para.text.get(idx + 1..).unwrap_or("").trim_start();
            let pad = " ".repeat(max_eq_col.saturating_sub(left.chars().count()) + 1);
            let new_text = format!("{left}{pad}= {right}");
            let pa = wordcraft_doc::Pos::body(p_idx, 0);
            let pb = wordcraft_doc::Pos::body(p_idx, para.len());
            let props = para.props_of_char(0).clone();
            if let Ok(at) = session.doc.delete_range(&pa, &pb) {
                let _ = session.doc.insert_text(&at, &new_text, &props);
            }
        }
    }
    session.relayout();
}

/// Case transformation operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseOp {
    Upper,
    Lower,
    Toggle,
}

/// Transform the case of text in the selection according to `op`.
pub fn apply_case_to_selection(session: &mut Session, op: CaseOp) {
    let (start, end) = session.sel.ordered();
    let start_p = start.path.0.first().copied().unwrap_or(0) as usize;
    let end_p = end.path.0.first().copied().unwrap_or(0) as usize;
    for p_idx in start_p..=end_p {
        let pos = wordcraft_doc::Pos::body(p_idx, 0);
        if let Some(para) = session.doc.para_at(&pos).cloned() {
            let lo = if p_idx == start_p { start.off.min(para.len()) } else { 0 };
            let hi = if p_idx == end_p { end.off.min(para.len()) } else { para.len() };
            if lo < hi
                && let Some(slice) = para.text.get(lo..hi)
            {
                let transformed: String = match op {
                    CaseOp::Upper => slice.to_uppercase(),
                    CaseOp::Lower => slice.to_lowercase(),
                    CaseOp::Toggle => slice
                        .chars()
                        .map(|c| if c.is_uppercase() { c.to_lowercase().next().unwrap_or(c) } else { c.to_uppercase().next().unwrap_or(c) })
                        .collect(),
                };
                let pa = wordcraft_doc::Pos::body(p_idx, lo);
                let pb = wordcraft_doc::Pos::body(p_idx, hi);
                let props = para.props_of_char(lo).clone();
                if let Ok(at) = session.doc.delete_range(&pa, &pb) {
                    let _ = session.doc.insert_text(&at, &transformed, &props);
                }
            }
        }
    }
    session.relayout();
    let _ = session.run("select.collapse", &json!({}));
}

fn repeat<F>(count: usize, mut f: F)
where
    F: FnMut() -> Result<Value, crate::CmdError>,
{
    for _ in 0..count.min(1000) {
        let _ = f();
    }
}
