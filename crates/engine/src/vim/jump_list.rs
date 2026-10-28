//! Jump list (<C-o>/<C-i>) and Change list (g;/g,) for WordCraft's Neovim engine.

use wordcraft_doc::Pos;

const MAX_JUMP_HISTORY: usize = 100;

/// Jump list tracking for `<C-o>` and `<C-i>`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JumpList {
    pub history: Vec<Pos>,
    pub current_idx: usize,
}

impl JumpList {
    /// Record a position before a major jump occurs (e.g. gg, G, /, ?, %, marks, flash).
    pub fn push(&mut self, pos: Pos) {
        // Avoid duplicate consecutive jumps to the exact same location
        if self.history.last() == Some(&pos) {
            return;
        }
        // If we were navigated back with <C-o>, truncate forward history on new jump
        if self.current_idx < self.history.len() {
            self.history.truncate(self.current_idx.saturating_add(1));
        }
        self.history.push(pos);
        if self.history.len() > MAX_JUMP_HISTORY {
            self.history.remove(0);
        }
        self.current_idx = self.history.len();
    }

    /// Jump to older position (<C-o>).
    pub fn jump_older(&mut self, current_pos: &Pos) -> Option<Pos> {
        if self.history.is_empty() {
            return None;
        }
        if self.current_idx >= self.history.len() {
            if self.history.last() != Some(current_pos) {
                self.history.push(current_pos.clone());
            }
            self.current_idx = self.history.len().saturating_sub(2);
            return self.history.get(self.current_idx).cloned();
        }
        if self.current_idx > 0 {
            self.current_idx = self.current_idx.saturating_sub(1);
            self.history.get(self.current_idx).cloned()
        } else {
            None
        }
    }

    /// Jump to newer position (<C-i>).
    pub fn jump_newer(&mut self) -> Option<Pos> {
        if self.history.is_empty() || self.current_idx.saturating_add(1) >= self.history.len() {
            return None;
        }
        self.current_idx = self.current_idx.saturating_add(1);
        self.history.get(self.current_idx).cloned()
    }
}

/// Change list tracking for `g;` and `g,`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeList {
    pub history: Vec<Pos>,
    pub current_idx: usize,
}

impl ChangeList {
    /// Record a position where document mutation occurred.
    pub fn push(&mut self, pos: Pos) {
        if self.history.last() == Some(&pos) {
            return;
        }
        self.history.push(pos);
        if self.history.len() > MAX_JUMP_HISTORY {
            self.history.remove(0);
        }
        self.current_idx = self.history.len();
    }

    /// Jump to older change location (`g;`).
    pub fn jump_older(&mut self) -> Option<Pos> {
        if self.history.is_empty() {
            return None;
        }
        if self.current_idx > 0 {
            self.current_idx = self.current_idx.saturating_sub(1);
            self.history.get(self.current_idx).cloned()
        } else {
            self.history.first().cloned()
        }
    }

    /// Jump to newer change location (`g,`).
    pub fn jump_newer(&mut self) -> Option<Pos> {
        if self.history.is_empty() {
            return None;
        }
        if self.current_idx.saturating_add(1) < self.history.len() {
            self.current_idx = self.current_idx.saturating_add(1);
            self.history.get(self.current_idx).cloned()
        } else {
            self.history.last().cloned()
        }
    }
}
