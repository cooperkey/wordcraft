//! Ex-commands execution for Vim mode in WordCraft.
//!
//! Handles `:w`, `:q`, `:wq`, `:x`, `:noh`, `:%s/pattern/replace/g`, etc.

use serde_json::json;
use crate::Session;

/// Result of executing an ex-command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExResult {
    /// Feedback message for the status bar.
    pub message: String,
    /// Whether the command failed.
    pub is_error: bool,
}

impl ExResult {
    pub fn ok(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            is_error: false,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            is_error: true,
        }
    }
}

/// Execute an ex-command line on the given session.
pub fn execute_ex(session: &mut Session, line: &str) -> ExResult {
    let raw = line.trim();
    let cmd = raw.strip_prefix(':').unwrap_or(raw).trim();

    if cmd.is_empty() {
        return ExResult::ok("");
    }

    // Save commands: :w, :write, :w <path>, :w!
    if cmd == "w" || cmd == "w!" || cmd == "write" || cmd == "write!" || cmd.starts_with("w ") || cmd.starts_with("write ") {
        let path = cmd
            .strip_prefix("write ")
            .or_else(|| cmd.strip_prefix("w "))
            .map(str::trim);

        let res = if let Some(p) = path {
            session.run("file.saveAs", &json!({"path": p}))
        } else {
            session.run("file.save", &json!({}))
        };

        return match res {
            Ok(_) => {
                let name = session.file_name.as_deref().unwrap_or("document");
                ExResult::ok(format!("\"{name}\" [written]"))
            }
            Err(e) => ExResult::err(format!("Save failed: {e}")),
        };
    }

    // Quit commands: :q, :quit, :q!
    if cmd == "q" || cmd == "quit" {
        if session.dirty {
            return ExResult::err("E37: No write since last change (use :q! to discard)");
        }
        return match session.run("file.close", &json!({})) {
            Ok(_) => ExResult::ok(""),
            Err(e) => ExResult::err(format!("Close failed: {e}")),
        };
    }

    if cmd == "q!" || cmd == "quit!" {
        // Discard unsaved changes and close
        session.dirty = false;
        return match session.run("file.close", &json!({})) {
            Ok(_) => ExResult::ok(""),
            Err(e) => ExResult::err(format!("Close failed: {e}")),
        };
    }

    // Save and quit: :wq, :x, :wq!
    if cmd == "wq" || cmd == "wq!" || cmd == "x" || cmd == "x!" {
        if let Err(e) = session.run("file.save", &json!({})) {
            return ExResult::err(format!("Save failed: {e}"));
        }
        return match session.run("file.close", &json!({})) {
            Ok(_) => ExResult::ok(""),
            Err(e) => ExResult::err(format!("Close failed: {e}")),
        };
    }

    // Clear search highlights: :noh, :nohlsearch
    if cmd == "noh" || cmd == "nohlsearch" {
        session.find.results.clear();
        session.find.highlights = None;
        session.find.highlight = false;
        session.relayout();
        return ExResult::ok("");
    }

    // Open file: :e <path>, :edit <path>
    if cmd.starts_with("e ") || cmd.starts_with("edit ") {
        let path = cmd
            .strip_prefix("edit ")
            .or_else(|| cmd.strip_prefix("e "))
            .map(str::trim)
            .unwrap_or("");
        if path.is_empty() {
            return ExResult::err("E471: Argument required");
        }
        return match session.run("file.open", &json!({"path": path})) {
            Ok(_) => ExResult::ok(format!("\"{path}\"")),
            Err(e) => ExResult::err(format!("Open failed: {e}")),
        };
    }

    // Line/paragraph navigation: :<number>
    if let Ok(line_num) = cmd.parse::<usize>() {
        let para_idx = line_num.saturating_sub(1);
        let body_len = session.doc.body.len();
        if body_len > 0 {
            let clamped = para_idx.min(body_len.saturating_sub(1));
            session.sel = crate::Selection::caret(wordcraft_doc::Pos::body(clamped, 0));
            return ExResult::ok(format!("Line {line_num}"));
        }
        return ExResult::ok("");
    }

    // Substitute commands: :s/old/new/flags or :%s/old/new/flags
    if cmd.starts_with("s") || cmd.starts_with("%s") {
        return execute_substitute(session, cmd);
    }

    ExResult::err(format!("E492: Not an editor command: :{cmd}"))
}

fn execute_substitute(session: &mut Session, cmd: &str) -> ExResult {
    let rest = if let Some(r) = cmd.strip_prefix("%s") {
        r
    } else if let Some(r) = cmd.strip_prefix('s') {
        r
    } else {
        return ExResult::err("Invalid substitution syntax");
    };

    let mut chars = rest.chars();
    let Some(delim) = chars.next() else {
        return ExResult::err("Missing delimiter for substitute (e.g. :%s/find/replace/g)");
    };

    let parts: Vec<&str> = rest[delim.len_utf8()..].split(delim).collect();
    if parts.len() < 2 {
        return ExResult::err(format!("Invalid substitution: expected 2 or 3 parts separated by '{delim}'"));
    }

    let pattern = parts[0];
    let replacement = parts[1];
    let flags = parts.get(2).copied().unwrap_or("");
    let is_case_insensitive = flags.contains('i');

    let mut opts = json!({
        "text": pattern,
        "with": replacement,
    });

    if is_case_insensitive {
        opts["matchCase"] = json!(false);
    } else {
        opts["matchCase"] = json!(true);
    }

    let res = session.run("edit.replaceAll", &opts);
    match res {
        Ok(val) => {
            let count = val.get("replaced").or_else(|| val.get("count")).and_then(|v| v.as_u64()).unwrap_or(0);
            ExResult::ok(format!("{count} substitutions on document"))
        }
        Err(e) => ExResult::err(format!("Substitute error: {e}")),
    }
}
