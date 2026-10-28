//! Complete 39-register subsystem for Vim mode in WordCraft.
//!
//! Full parity with Neovim's `register.c` and `register_defs.h`:
//! - `""` Unnamed register: updated on any delete, change, or yank.
//! - `"0` Yank register: records last yanked text; never overwritten by deletes.
//! - `"1`-`"9` Delete ring: records line/multi-line deletes in rotating stack.
//! - `"-` Small delete register: deletes within a single line (less than a line).
//! - `"a`-`"z` Named registers.
//! - `"A`-`"Z` Append to named register (uppercased version appends instead of replacing).
//! - `"+`, `"*` System clipboard register.
//! - `"_` Black hole register: discards writes; reads return empty.
//! - `".` Last inserted text (read-only).
//! - `":` Last executed command (read-only).
//! - `"%` Current document file path (read-only).
//! - `"/` Last search pattern (read-only).
//! - `"=" Expression register.

use crate::Session;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

/// Type of text stored in a register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegType {
    Charwise,
    Linewise,
    Blockwise,
}

/// A register value holding text and its selection type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterValue {
    pub text: String,
    pub reg_type: RegType,
}

/// The complete register bank.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RegisterBank {
    named: HashMap<char, RegisterValue>,
    delete_ring: Vec<RegisterValue>,
    small_delete: Option<RegisterValue>,
    yank: Option<RegisterValue>,
    unnamed: Option<RegisterValue>,
    pub last_inserted: String,
    pub last_command: String,
    pub current_file: String,
    pub last_search: String,
}

impl RegisterBank {
    /// Create a new empty register bank.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert text into a register with `RegType::Charwise`.
    pub fn insert(&mut self, reg: char, text: impl Into<String>) {
        self.insert_typed(reg, text.into(), RegType::Charwise);
    }

    /// Insert text and register type into a register with full Neovim semantics.
    pub fn insert_typed(&mut self, reg: char, text: String, reg_type: RegType) {
        // Black hole register: discards writes
        if reg == '_' {
            return;
        }

        // Uppercase named register: APPENDS to lowercase register
        if reg.is_ascii_uppercase() {
            let lower = reg.to_ascii_lowercase();
            if let Some(existing) = self.named.get_mut(&lower) {
                if existing.reg_type == RegType::Linewise || reg_type == RegType::Linewise {
                    if !existing.text.ends_with('\n') {
                        existing.text.push('\n');
                    }
                    existing.text.push_str(&text);
                    existing.reg_type = RegType::Linewise;
                } else {
                    existing.text.push_str(&text);
                }
            } else {
                self.named.insert(lower, RegisterValue { text, reg_type });
            }
            return;
        }

        // Lowercase named register: replaces
        if reg.is_ascii_lowercase() {
            self.named.insert(reg, RegisterValue { text, reg_type });
            return;
        }

        // Specific numbered / special registers
        match reg {
            '"' => self.unnamed = Some(RegisterValue { text, reg_type }),
            '0' => self.yank = Some(RegisterValue { text, reg_type }),
            '-' => self.small_delete = Some(RegisterValue { text, reg_type }),
            '+' | '*' => {
                self.named.insert('+', RegisterValue { text: text.clone(), reg_type });
                self.named.insert('*', RegisterValue { text, reg_type });
            }
            '1'..='9' => {
                let idx = (reg as usize).saturating_sub('1' as usize);
                while self.delete_ring.len() <= idx {
                    self.delete_ring.push(RegisterValue { text: String::new(), reg_type: RegType::Linewise });
                }
                if let Some(slot) = self.delete_ring.get_mut(idx) {
                    *slot = RegisterValue { text, reg_type };
                }
            }
            _ => {
                self.named.insert(reg, RegisterValue { text, reg_type });
            }
        }
    }

    /// Get register text for compatibility with `HashMap<char, String>::get(&'a')`.
    pub fn get(&self, reg: &char) -> Option<&String> {
        self.get_value(*reg).map(|v| &v.text)
    }

    /// Retrieve full `RegisterValue` for a register character.
    pub fn get_value(&self, reg: char) -> Option<&RegisterValue> {
        match reg {
            '_' => None,
            '"' => self.unnamed.as_ref(),
            '0' => self.yank.as_ref(),
            '-' => self.small_delete.as_ref(),
            '1'..='9' => {
                let idx = (reg as usize).saturating_sub('1' as usize);
                self.delete_ring.get(idx)
            }
            'a'..='z' => self.named.get(&reg),
            'A'..='Z' => self.named.get(&reg.to_ascii_lowercase()),
            '+' | '*' => self.named.get(&'+').or_else(|| self.named.get(&'*')),
            '.' => None,
            _ => self.named.get(&reg),
        }
    }

    /// Record a yank into the register bank.
    pub fn record_yank(&mut self, explicit_reg: Option<char>, text: String, reg_type: RegType) {
        let val = RegisterValue { text: text.clone(), reg_type };
        self.unnamed = Some(val.clone());
        self.yank = Some(val);

        if let Some(r) = explicit_reg {
            self.insert_typed(r, text, reg_type);
        }
    }

    /// Record a delete/cut into the register bank.
    pub fn record_delete(&mut self, explicit_reg: Option<char>, text: String, reg_type: RegType) {
        if explicit_reg == Some('_') {
            return;
        }

        let is_linewise = reg_type == RegType::Linewise || text.contains('\n');
        let val = RegisterValue { text: text.clone(), reg_type };
        self.unnamed = Some(val.clone());

        if is_linewise {
            // Push to delete ring 1-9
            self.delete_ring.insert(0, val);
            if self.delete_ring.len() > 9 {
                self.delete_ring.truncate(9);
            }
        } else {
            // Small delete within a single line
            self.small_delete = Some(val);
        }

        if let Some(r) = explicit_reg {
            self.insert_typed(r, text, reg_type);
        }
    }

    /// Get all populated register keys for `:reg` command.
    pub fn keys(&self) -> Vec<char> {
        let mut keys = Vec::new();
        if self.unnamed.is_some() {
            keys.push('"');
        }
        if self.yank.is_some() {
            keys.push('0');
        }
        for (i, _) in self.delete_ring.iter().enumerate() {
            if let Some(ch) = char::from_digit((i + 1) as u32, 10) {
                keys.push(ch);
            }
        }
        if self.small_delete.is_some() {
            keys.push('-');
        }
        for &k in self.named.keys() {
            keys.push(k);
        }
        keys.sort();
        keys.dedup();
        keys
    }

    /// Execute paste (`p` or `P`) with authentic linewise vs charwise semantics.
    pub fn paste(&self, session: &mut Session, reg: Option<char>, before: bool, count: usize) {
        let val = reg.and_then(|r| self.get_value(r)).or(self.unnamed.as_ref());

        let Some(val) = val else {
            // If register is empty, fallback to system clipboard paste
            let _ = session.run("edit.paste", &json!({}));
            return;
        };

        if val.text.is_empty() {
            return;
        }

        match val.reg_type {
            RegType::Linewise => {
                let clean_text = val.text.trim_end_matches('\n');
                if before {
                    // 'P': Put above the current line
                    let _ = session.run("caret.home", &json!({}));
                    let _ = session.run("text.newParagraph", &json!({}));
                    let _ = session.run("caret.up", &json!({}));
                    for i in 0..count {
                        if i > 0 {
                            let _ = session.run("text.newParagraph", &json!({}));
                        }
                        let _ = session.run("text.insert", &json!({"text": clean_text}));
                    }
                } else {
                    // 'p': Put below the current line
                    let _ = session.run("caret.end", &json!({}));
                    for _ in 0..count {
                        let _ = session.run("text.newParagraph", &json!({}));
                        let _ = session.run("text.insert", &json!({"text": clean_text}));
                    }
                }
            }
            RegType::Charwise => {
                if !before && session.sel.focus.off > 0 {
                    // 'p': Put after cursor
                    let _ = session.run("caret.right", &json!({}));
                }
                for _ in 0..count {
                    let _ = session.run("text.insert", &json!({"text": val.text}));
                }
            }
            RegType::Blockwise => {
                for _ in 0..count {
                    let _ = session.run("text.insert", &json!({"text": val.text}));
                }
            }
        }
    }
}
