//! Implementation of Vim's dot repeat (`.`) command.

use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::Session;
use super::motion::{CharFind, execute_char_find};
use super::text_object::find_text_object;
use super::surround::{change_surround, delete_surround, wrap_selection};

/// A recorded modifying edit that can be repeated with `.`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LastVimChange {
    /// Delete character under caret (`x`).
    DeleteChar,
    /// Delete entire line (`dd`).
    DeleteLine,
    /// Delete motion (e.g. `dw`, `d$`).
    DeleteMotion(String),
    /// Delete text object (e.g. `diw`, `da"`).
    DeleteTextObject { target: char, inner: bool },
    /// Delete inline character find (e.g. `df.`, `dt)`).
    DeleteCharFind(CharFind),
    /// Change with inserted text (e.g. `cw` or `ci"` + text + Esc).
    Change {
        target: String,
        text: String,
    },
    /// Paste clipboard (`p`).
    Paste,
    /// Wrap selection / text object with delimiter (`ys{obj}{char}`).
    SurroundWrap { target: char, inner: bool, delim: char },
    /// Delete surround (`ds{char}`).
    SurroundDelete(char),
    /// Change surround (`cs{old}{new}`).
    SurroundChange { old: char, new: char },
    /// Indent line / paragraph (`>`).
    Indent,
    /// Outdent line / paragraph (`<`).
    Outdent,
    /// Join line below (`J`).
    Join,
}

impl LastVimChange {
    /// Execute this recorded change on the given session.
    pub fn replay(&self, session: &mut Session) -> bool {
        match self {
            Self::DeleteChar => {
                let _ = session.run("text.delete", &json!({}));
                true
            }
            Self::DeleteLine => {
                let _ = session.run("select.line", &json!({}));
                let _ = session.run("edit.cut", &json!({}));
                true
            }
            Self::DeleteMotion(motion) => {
                let ext = json!({"extend": true});
                let _ = session.run(motion, &ext);
                let _ = session.run("edit.cut", &json!({}));
                true
            }
            Self::DeleteTextObject { target, inner } => {
                if let Some((start, end)) = find_text_object(session, *target, *inner) {
                    session.sel = crate::Selection { anchor: start, focus: end };
                    let _ = session.run("edit.cut", &json!({}));
                    true
                } else {
                    false
                }
            }
            Self::DeleteCharFind(find) => {
                if execute_char_find(session, *find, true) {
                    let _ = session.run("edit.cut", &json!({}));
                    true
                } else {
                    false
                }
            }
            Self::Change { target, text } => {
                let ext = json!({"extend": true});
                let _ = session.run(target, &ext);
                let _ = session.run("edit.cut", &json!({}));
                if !text.is_empty() {
                    let _ = session.run("text.insert", &json!({"text": text}));
                }
                true
            }
            Self::Paste => {
                let _ = session.run("edit.paste", &json!({}));
                true
            }
            Self::SurroundWrap { target, inner, delim } => {
                if let Some((start, end)) = find_text_object(session, *target, *inner) {
                    session.sel = crate::Selection { anchor: start, focus: end };
                    wrap_selection(session, *delim)
                } else {
                    false
                }
            }
            Self::SurroundDelete(c) => delete_surround(session, *c),
            Self::SurroundChange { old, new } => change_surround(session, *old, *new),
            Self::Indent => {
                let _ = session.run("para.indent", &json!({}));
                true
            }
            Self::Outdent => {
                let _ = session.run("para.outdent", &json!({}));
                true
            }
            Self::Join => {
                let _ = session.run("caret.end", &json!({}));
                let _ = session.run("text.delete", &json!({}));
                true
            }
        }
    }
}
