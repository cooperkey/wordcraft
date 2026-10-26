//! Native modal Vim editing engine for WordCraft.
//!
//! Provides a pure-Rust modal state machine (Normal, Insert, Visual, VisualLine, Command)
//! that maps Vim motions, operators, text objects, and counts directly into WordCraft's command registry.

pub mod config;
pub mod ex;
pub mod text_object;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use config::VimConfig;
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
                    self.count = None;
                    return true;
                }
                "U" => {
                    let _ = session.run("caret.pageUp", &json!({}));
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

        // Two-character motion sequences
        if !self.pending.is_empty() {
            let p = std::mem::take(&mut self.pending);
            if p == "g" && input_char == Some('g') {
                let _ = session.run("caret.docStart", &json!({}));
                self.count = None;
                return true;
            }
            self.count = None;
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
            Some('x') => {
                repeat(cnt, || session.run("text.delete", &json!({})));
                true
            }
            Some('u') => {
                let _ = session.run("edit.undo", &json!({}));
                true
            }
            Some('p') => {
                let _ = session.run("edit.paste", &json!({}));
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
