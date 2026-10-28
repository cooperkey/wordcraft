//! Unicode Hex Input Layer for Vim mode (`<C-q>`).
//!
//! Provides a 4-digit hexadecimal input interface for inserting Unicode characters.
//! Users type 1-4 hex digits (0-9, a-f) and see a live preview of the resulting character.

use crate::Session;
use serde_json::json;

/// Maximum hex digits allowed in the Unicode input layer.
pub const MAX_HEX_DIGITS: usize = 4;

/// Check if a character is a valid hex digit (0-9, a-f, A-F).
pub fn is_hex_digit(c: char) -> bool {
    c.is_ascii_hexdigit()
}

/// Parse a hex string (1-4 chars) into a Unicode character, if valid.
pub fn parse_hex(hex: &str) -> Option<char> {
    if hex.is_empty() || hex.len() > MAX_HEX_DIGITS {
        return None;
    }
    let code = u32::from_str_radix(hex, 16).ok()?;
    char::from_u32(code)
}

/// Format the hex input layer display string.
/// Shows: `UNICODE u{digits}  ->  {preview}`
/// Pads digits with underscores to fill 4 slots.
pub fn format_display(hex: &str) -> String {
    let mut padded = hex.to_string();
    while padded.len() < MAX_HEX_DIGITS {
        padded.push('_');
    }
    let preview = parse_hex(hex).map(|c| c.to_string()).unwrap_or_default();
    if preview.is_empty() {
        format!("UNICODE u{}  (Enter: insert, Esc: cancel)", padded)
    } else {
        format!("UNICODE u{}  ->  {}  (Enter: insert, Esc: cancel)", padded, preview)
    }
}

/// Result of processing a hex input key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HexKeyResult {
    /// Key was handled, stay in hex mode.
    Continue,
    /// Character was inserted, exit hex mode.
    Inserted,
    /// User cancelled, exit hex mode.
    Cancel,
}

/// Handle a keystroke in the Unicode Hex Input Layer.
pub fn handle_hex_key(session: &mut Session, hex_buf: &mut String, key_name: &str, input_char: Option<char>) -> HexKeyResult {
    match key_name {
        "Escape" => HexKeyResult::Cancel,
        "Enter" | "Return" => {
            if let Some(ch) = parse_hex(hex_buf) {
                let _ = session.run("text.insert", &json!({"text": ch.to_string()}));
                HexKeyResult::Inserted
            } else {
                HexKeyResult::Cancel
            }
        }
        "Backspace" => {
            hex_buf.pop();
            if hex_buf.is_empty() { HexKeyResult::Cancel } else { HexKeyResult::Continue }
        }
        _ => {
            if let Some(c) = input_char {
                if is_hex_digit(c) && hex_buf.len() < MAX_HEX_DIGITS {
                    hex_buf.push(c.to_ascii_lowercase());
                    if hex_buf.len() == MAX_HEX_DIGITS {
                        if let Some(ch) = parse_hex(hex_buf) {
                            let _ = session.run("text.insert", &json!({"text": ch.to_string()}));
                            return HexKeyResult::Inserted;
                        } else {
                            return HexKeyResult::Continue;
                        }
                    }
                    HexKeyResult::Continue
                } else {
                    HexKeyResult::Continue
                }
            } else {
                HexKeyResult::Continue
            }
        }
    }
}
