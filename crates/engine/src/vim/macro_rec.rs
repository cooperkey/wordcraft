//! Macro recording and playback for WordCraft's Neovim engine.

use std::collections::HashMap;

/// A single recorded keystroke in a Vim macro.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VimMacroKey {
    pub key_name: String,
    pub text: Option<String>,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Macro recording and playback state.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VimMacroState {
    /// Currently recording register (e.g. Some('a')).
    pub recording: Option<char>,
    /// Keystrokes collected in the active recording session.
    pub current_buffer: Vec<VimMacroKey>,
    /// Saved macros indexed by register character ('a'..='z', '0'..='9', etc.).
    pub macros: HashMap<char, Vec<VimMacroKey>>,
    /// The last executed macro register character (for `@@`).
    pub last_executed: Option<char>,
    /// Guard flag to prevent infinite recursive macro replay loops.
    pub is_replaying: bool,
}

impl VimMacroState {
    /// Start recording into the given register.
    pub fn start_recording(&mut self, reg: char) {
        self.recording = Some(reg.to_ascii_lowercase());
        self.current_buffer.clear();
    }

    /// Stop recording and store the buffer into the register. Returns the register if stopped.
    pub fn stop_recording(&mut self) -> Option<char> {
        let reg = self.recording.take()?;
        // The keystroke that stopped the recording ('q') should not be stored.
        self.macros.insert(reg, std::mem::take(&mut self.current_buffer));
        Some(reg)
    }

    /// Record a keystroke if recording is active.
    pub fn record(&mut self, key_name: &str, text: Option<&str>, ctrl: bool, alt: bool, shift: bool) {
        if self.recording.is_some() && !self.is_replaying {
            self.current_buffer.push(VimMacroKey { key_name: key_name.to_string(), text: text.map(|s| s.to_string()), ctrl, alt, shift });
        }
    }

    /// Get macro keystrokes for a register.
    pub fn get(&self, reg: char) -> Option<&[VimMacroKey]> {
        self.macros.get(&reg.to_ascii_lowercase()).map(|v| v.as_slice())
    }
}
