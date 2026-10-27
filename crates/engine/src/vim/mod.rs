//! Native modal Vim editing engine for WordCraft.
//!
//! Provides a pure-Rust modal state machine (Normal, Insert, Visual, VisualLine, Command)
//! that maps Vim motions, operators, text objects, and counts directly into WordCraft's command registry.

pub mod config;
pub mod ex;
pub mod flash;
pub mod motion;
pub mod repeat;
pub mod surround;
pub mod text_object;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use config::VimConfig;
pub use motion::CharFind;
pub use repeat::LastVimChange;
use crate::{Selection, Session};

/// An operator waiting for a motion or text object (`d`, `c`, `y`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VimOp {
    /// Delete / cut range.
    Delete,
    /// Change range (delete + enter Insert mode).
    Change,
    /// Yank / copy range to clipboard.
    Yank,
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
    /// Character-wise visual selection.
    Visual,
    /// Line/paragraph-wise visual selection.
    VisualLine,
    /// Ex-command line mode (`:`).
    Command,
}

impl VimMode {
    /// Short display badge for the status bar.
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Normal => "-- NORMAL --",
            Self::Insert => "-- INSERT --",
            Self::Visual => "-- VISUAL --",
            Self::VisualLine => "-- VISUAL LINE --",
            Self::Command => "-- COMMAND --",
        }
    }
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
    /// Insert text buffer recorded during Insert mode for repeating change operations.
    pub insert_buf: String,
}

impl Default for VimState {
    fn default() -> Self {
        let config = VimConfig::load();
        let mode = if config.default_mode == "insert" {
            VimMode::Insert
        } else {
            VimMode::Normal
        };
        Self {
            enabled: false,
            mode,
            count: None,
            pending: String::new(),
            pending_op: None,
            config,
            cmd_input: String::new(),
            status_msg: None,
            last_char_find: None,
            last_change: None,
            viewport_req: None,
            insert_buf: String::new(),
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
            self.mode = if self.config.default_mode == "insert" {
                VimMode::Insert
            } else {
                VimMode::Normal
            };
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
        self.cmd_input.clear();
    }

    /// Take and clear the accumulated count (defaults to 1 if none).
    pub fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1).clamp(1, 1000)
    }

    /// Handle a keystroke. Returns true if the key was consumed by Vim mode.
    pub fn handle_key(
        &mut self,
        session: &mut Session,
        key_name: &str,
        text: Option<&str>,
        ctrl: bool,
        alt: bool,
        shift: bool,
    ) -> bool {
        if !self.enabled {
            return false;
        }

        match self.mode {
            VimMode::Insert => self.handle_insert(session, key_name, ctrl),
            VimMode::Normal => self.handle_normal(session, key_name, text, ctrl, alt, shift),
            VimMode::Visual | VimMode::VisualLine => self.handle_visual(session, key_name, text, ctrl, shift),
            VimMode::Command => self.handle_command(session, key_name, text),
        }
    }

    fn handle_insert(&mut self, session: &mut Session, key_name: &str, ctrl: bool) -> bool {
        if key_name == "Escape" || (ctrl && key_name == "[") {
            // Step caret left by 1 character, returning to Normal mode (classic Vim behavior).
            let _ = session.run("caret.left", &json!({}));
            self.mode = VimMode::Normal;
            self.reset_pending();
            return true;
        }
        false
    }

    fn handle_normal(
        &mut self,
        session: &mut Session,
        key_name: &str,
        text: Option<&str>,
        ctrl: bool,
        alt: bool,
        _shift: bool,
    ) -> bool {
        // Escape clears pending sequence, count, operators, and selections
        if key_name == "Escape" {
            self.reset_pending();
            self.status_msg = None;
            let _ = session.run("select.collapse", &json!({}));
            return true;
        }

        let input_char = text.and_then(|t| t.chars().next());

        // Numeric count accumulator (1-9, or 0 if count already started)
        if !ctrl
            && !alt
            && self.pending.is_empty()
            && self.pending_op.is_none()
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
                    '1' => { let _ = session.run("style.heading1", &json!({})); self.reset_pending(); return true; }
                    '2' => { let _ = session.run("style.heading2", &json!({})); self.reset_pending(); return true; }
                    '3' => { let _ = session.run("style.heading3", &json!({})); self.reset_pending(); return true; }
                    '4' => { let _ = session.run("style.heading4", &json!({})); self.reset_pending(); return true; }
                    '5' => { let _ = session.run("style.heading5", &json!({})); self.reset_pending(); return true; }
                    '6' => { let _ = session.run("style.heading6", &json!({})); self.reset_pending(); return true; }
                    'x' => { let _ = session.run("list.bullet", &json!({})); self.reset_pending(); return true; }
                    _ => {}
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
            if p == "g" && input_char == Some('g') {
                let _ = session.run("caret.docStart", &json!({}));
                self.reset_pending();
                return true;
            }
            if p == "z" {
                match input_char {
                    Some('z') => { self.viewport_req = Some("center"); }
                    Some('t') => { self.viewport_req = Some("top"); }
                    Some('b') => { self.viewport_req = Some("bottom"); }
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
            if p.starts_with('s') && p.len() == 2 {
                if let Some(c2) = input_char {
                    let c1 = p.chars().nth(1).unwrap_or(' ');
                    flash::flash_jump(session, c1, c2);
                }
                self.reset_pending();
                return true;
            } else if p == "s" && let Some(c1) = input_char {
                self.pending = format!("s{c1}");
                return true;
            }

            self.reset_pending();
            return true;
        }

        // Initiating operators: d, c, y
        match input_char {
            Some('d') => {
                self.pending_op = Some(VimOp::Delete);
                return true;
            }
            Some('c') => {
                self.pending_op = Some(VimOp::Change);
                return true;
            }
            Some('y') => {
                self.pending_op = Some(VimOp::Yank);
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
                repeat(cnt, || session.run("caret.wordRight", &json!({})));
                true
            }
            Some('b') => {
                repeat(cnt, || session.run("caret.wordLeft", &json!({})));
                true
            }
            Some('e') => {
                repeat(cnt, || session.run("caret.wordRight", &json!({})));
                true
            }
            Some('0') => {
                let _ = session.run("caret.home", &json!({}));
                true
            }
            Some('$') => {
                let _ = session.run("caret.end", &json!({}));
                true
            }
            Some('^') => {
                let _ = session.run("caret.home", &json!({}));
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
            Some('g') => {
                self.pending = "g".to_string();
                true
            }
            Some('G') => {
                let _ = session.run("caret.docEnd", &json!({}));
                true
            }
            Some('z') => {
                self.pending = "z".to_string();
                true
            }
            Some('s') => {
                self.pending = "s".to_string();
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
                repeat(cnt, || session.run("text.delete", &json!({})));
                self.last_change = Some(LastVimChange::DeleteChar);
                true
            }
            Some('u') => {
                let _ = session.run("edit.undo", &json!({}));
                true
            }
            Some('p') => {
                let _ = session.run("edit.paste", &json!({}));
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
                let _ = session.run("caret.home", &json!({}));
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
                let _ = session.run("caret.left", &json!({}));
                let _ = session.run("text.newParagraph", &json!({}));
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

    fn handle_operator_pending(
        &mut self,
        session: &mut Session,
        op: VimOp,
        key_name: &str,
        input_char: Option<char>,
    ) -> bool {
        // Double operator: dd, cc, yy (applies to line)
        let is_double = matches!((op, input_char), (VimOp::Delete, Some('d')) | (VimOp::Change, Some('c')) | (VimOp::Yank, Some('y')));

        if is_double {
            let cnt = self.take_count();
            for _ in 0..cnt {
                let _ = session.run("select.line", &json!({}));
                match op {
                    VimOp::Delete => {
                        let _ = session.run("edit.cut", &json!({}));
                        self.last_change = Some(LastVimChange::DeleteLine);
                    }
                    VimOp::Change => {
                        let _ = session.run("edit.cut", &json!({}));
                        self.mode = VimMode::Insert;
                    }
                    VimOp::Yank => {
                        let _ = session.run("edit.copy", &json!({}));
                        let _ = session.run("select.collapse", &json!({}));
                    }
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
        let cnt = self.take_count();
        let ran = match input_char {
            Some('w') => {
                repeat(cnt, || session.run("caret.wordRight", &ext));
                true
            }
            Some('b') => {
                repeat(cnt, || session.run("caret.wordLeft", &ext));
                true
            }
            Some('e') => {
                repeat(cnt, || session.run("caret.wordRight", &ext));
                true
            }
            Some('$') => {
                let _ = session.run("caret.end", &ext);
                true
            }
            Some('0') | Some('^') => {
                let _ = session.run("caret.home", &ext);
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
            if op == VimOp::Delete && let Some(m) = input_char {
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
        match op {
            VimOp::Delete => {
                let _ = session.run("edit.cut", &json!({}));
            }
            VimOp::Change => {
                let _ = session.run("edit.cut", &json!({}));
                self.mode = VimMode::Insert;
            }
            VimOp::Yank => {
                let _ = session.run("edit.copy", &json!({}));
                let _ = session.run("select.collapse", &json!({}));
            }
        }
    }

    fn handle_visual(
        &mut self,
        session: &mut Session,
        key_name: &str,
        text: Option<&str>,
        _ctrl: bool,
        _shift: bool,
    ) -> bool {
        if key_name == "Escape" {
            let _ = session.run("select.collapse", &json!({}));
            self.mode = VimMode::Normal;
            self.reset_pending();
            return true;
        }

        let input_char = text.and_then(|t| t.chars().next());
        let ext = json!({"extend": true});

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

        if self.pending.is_empty() && let Some(c) = input_char && self.config.leader == c.to_string() {
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

        if self.pending == "S" && let Some(delimiter) = input_char {
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

        match input_char {
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
                    session.sel = Selection {
                        anchor: wordcraft_doc::Pos::body(new_p, 0),
                        focus: wordcraft_doc::Pos::body(new_p, len),
                    };
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
                    session.sel = Selection {
                        anchor: wordcraft_doc::Pos::body(new_p, 0),
                        focus: wordcraft_doc::Pos::body(new_p, len),
                    };
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
            Some('y') => {
                let _ = session.run("edit.copy", &json!({}));
                let _ = session.run("select.collapse", &json!({}));
                self.mode = VimMode::Normal;
                true
            }
            Some('d') | Some('x') => {
                let _ = session.run("edit.cut", &json!({}));
                self.mode = VimMode::Normal;
                true
            }
            Some('c') => {
                let _ = session.run("edit.cut", &json!({}));
                self.mode = VimMode::Insert;
                true
            }
            Some('p') => {
                let _ = session.run("edit.paste", &json!({}));
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
        }
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
}

fn repeat<F>(count: usize, mut f: F)
where
    F: FnMut() -> Result<Value, crate::CmdError>,
{
    for _ in 0..count.min(1000) {
        let _ = f();
    }
}
