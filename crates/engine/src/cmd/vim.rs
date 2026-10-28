//! Vim modal editing commands.

use serde_json::{Value, json};

use crate::vim::VimMode;
use crate::{CmdError, CmdResult, CommandSpec, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("vim.toggle", "Toggle Vim Mode", "Tools › Vim Mode", toggle).key("Mod+Alt+V").params(r#"{"value"?: bool}"#).pure(),
        CommandSpec::new("vim.setMode", "Set Vim Mode", "Tools › Vim Mode", set_mode)
            .params(r#"{"mode": "normal" | "insert" | "visual" | "visualLine"}"#)
            .pure(),
    ]
}

fn toggle(s: &mut Session, v: &Value) -> CmdResult {
    let new_val = p::bool(v, "value").unwrap_or(!s.vim.enabled);
    s.vim.enabled = new_val;
    s.prefs.vim_mode = new_val;
    if new_val {
        s.vim.mode = VimMode::Normal;
        s.vim.pending.clear();
        s.vim.count = None;
    }
    Ok(json!({
        "enabled": s.vim.enabled,
        "mode": match s.vim.mode {
            VimMode::Normal => "normal",
            VimMode::Insert => "insert",
            VimMode::Visual => "visual",
            VimMode::VisualLine => "visualLine",
            VimMode::VisualBlock => "visualBlock",
            VimMode::Command => "command",
        }
    }))
}

fn set_mode(s: &mut Session, v: &Value) -> CmdResult {
    let mode_str = p::str(v, "mode").ok_or_else(|| CmdError::Params("missing 'mode'".into()))?;
    let mode = match mode_str {
        "normal" => VimMode::Normal,
        "insert" => VimMode::Insert,
        "visual" => VimMode::Visual,
        "visualLine" => VimMode::VisualLine,
        "visualBlock" => VimMode::VisualBlock,
        "command" => VimMode::Command,
        _ => return Err(CmdError::Params(format!("unknown mode '{mode_str}'"))),
    };
    s.vim.set_mode(mode);
    Ok(json!({"mode": mode_str}))
}
