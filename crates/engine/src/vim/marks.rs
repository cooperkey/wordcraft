//! Complete Marks subsystem for Vim mode in WordCraft.
//!
//! Full parity with Neovim's `mark.c`:
//! - Local marks: `m{a-z}`
//! - Global marks: `m{A-Z}`
//! - Jump mark: `''` and ` `` `
//! - Visual marks: `'<` and `'>`
//! - Change marks: `'[` and `']`
//! - Last edit: `'.`
//! - Last insert exit: `'^`

use std::collections::HashMap;
use wordcraft_doc::Pos;

/// Mark storage table for local, global, and special marks.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MarkTable {
    pub local_marks: HashMap<char, Pos>,
    pub global_marks: HashMap<char, Pos>,
    pub last_jump: Option<Pos>,
    pub visual_start: Option<Pos>,
    pub visual_end: Option<Pos>,
    pub change_start: Option<Pos>,
    pub change_end: Option<Pos>,
    pub last_edit: Option<Pos>,
    pub last_insert_exit: Option<Pos>,
}

impl MarkTable {
    /// Create a new mark table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a mark at a position.
    pub fn set(&mut self, mark: char, pos: Pos) {
        if mark.is_ascii_lowercase() {
            self.local_marks.insert(mark, pos);
        } else if mark.is_ascii_uppercase() {
            self.global_marks.insert(mark, pos);
        } else {
            match mark {
                '<' => self.visual_start = Some(pos),
                '>' => self.visual_end = Some(pos),
                '[' => self.change_start = Some(pos),
                ']' => self.change_end = Some(pos),
                '.' => self.last_edit = Some(pos),
                '^' => self.last_insert_exit = Some(pos),
                _ => {}
            }
        }
    }

    /// Retrieve the position of a mark.
    pub fn get(&self, mark: &char) -> Option<Pos> {
        if mark.is_ascii_lowercase() {
            self.local_marks.get(mark).cloned()
        } else if mark.is_ascii_uppercase() {
            self.global_marks.get(mark).cloned()
        } else {
            match mark {
                '\'' | '`' => self.last_jump.clone(),
                '<' => self.visual_start.clone(),
                '>' => self.visual_end.clone(),
                '[' => self.change_start.clone(),
                ']' => self.change_end.clone(),
                '.' => self.last_edit.clone(),
                '^' => self.last_insert_exit.clone(),
                _ => None,
            }
        }
    }

    /// Compatibility helper matching `HashMap::insert`.
    pub fn insert(&mut self, mark: char, pos: Pos) {
        self.set(mark, pos);
    }

    /// Compatibility helper matching `HashMap::keys`.
    pub fn keys(&self) -> Vec<char> {
        let mut keys = Vec::new();
        for &k in self.local_marks.keys() {
            keys.push(k);
        }
        for &k in self.global_marks.keys() {
            keys.push(k);
        }
        if self.last_jump.is_some() {
            keys.push('\'');
        }
        if self.visual_start.is_some() {
            keys.push('<');
        }
        if self.visual_end.is_some() {
            keys.push('>');
        }
        if self.change_start.is_some() {
            keys.push('[');
        }
        if self.change_end.is_some() {
            keys.push(']');
        }
        if self.last_edit.is_some() {
            keys.push('.');
        }
        if self.last_insert_exit.is_some() {
            keys.push('^');
        }
        keys.sort();
        keys.dedup();
        keys
    }
}
