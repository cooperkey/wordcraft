//! Ex-commands execution for Vim mode in WordCraft.
//!
//! Handles `:w`, `:q`, `:wq`, `:x`, `:noh`, `:%s/pattern/replace/g`, etc.

use crate::Session;
use serde_json::json;

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
        Self { message: msg.into(), is_error: false }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self { message: msg.into(), is_error: true }
    }
}

/// Execute an ex-command line on the given session.
pub fn execute_ex(session: &mut Session, line: &str) -> ExResult {
    let raw = line.trim();
    let cmd = raw.strip_prefix(':').unwrap_or(raw).trim();

    if cmd.is_empty() {
        return ExResult::ok("");
    }

    // Visual range commands: :'<,'>...
    if let Some(rest) = cmd.strip_prefix("'<,'>") {
        let range_cmd = rest.trim();
        if range_cmd == "d" {
            let _ = session.run("edit.cut", &json!({}));
            return ExResult::ok("");
        }
        if range_cmd == "y" {
            let _ = session.run("edit.copy", &json!({}));
            let _ = session.run("select.collapse", &json!({}));
            return ExResult::ok("");
        }
        if range_cmd.starts_with('s') {
            return execute_substitute_range(session, range_cmd);
        }
    }

    // Save commands: :w, :write, :w <path>, :w!
    if cmd == "w" || cmd == "w!" || cmd == "write" || cmd == "write!" || cmd.starts_with("w ") || cmd.starts_with("write ") {
        let path = cmd.strip_prefix("write ").or_else(|| cmd.strip_prefix("w ")).map(str::trim);

        let res = if let Some(p) = path { session.run("file.saveAs", &json!({"path": p})) } else { session.run("file.save", &json!({})) };

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

    // Display registers: :reg, :registers
    if cmd == "reg" || cmd == "registers" {
        let vim = &session.vim;
        let mut lines = Vec::new();
        // Show clipboard register (system)
        lines.push("--- Registers ---".to_string());
        // Show named registers
        let mut keys: Vec<char> = vim.registers.keys().copied().collect();
        keys.sort();
        for k in keys {
            if let Some(val) = vim.registers.get(&k) {
                let display = if val.len() > 40 { format!("{}...", val.get(..40).unwrap_or(val)) } else { val.clone() };
                lines.push(format!("  \"{k}  {display}"));
            }
        }
        if lines.len() == 1 {
            lines.push("  (empty)".to_string());
        }
        return ExResult::ok(lines.join("\n"));
    }

    // Display marks: :marks
    if cmd == "marks" {
        let vim = &session.vim;
        let mut lines = Vec::new();
        lines.push("--- Marks ---".to_string());
        let mut keys: Vec<char> = vim.marks.keys().copied().collect();
        keys.sort();
        for k in keys {
            if let Some(pos) = vim.marks.get(&k) {
                let line = pos.path.0.first().copied().unwrap_or(0) as usize + 1;
                let col = pos.off;
                lines.push(format!("  '{k}  line {line} col {col}"));
            }
        }
        if lines.len() == 1 {
            lines.push("  (no marks set)".to_string());
        }
        return ExResult::ok(lines.join("\n"));
    }

    // Jump list inspection: :jumps
    if cmd == "jumps" {
        let mut lines = vec!["jump line  col text".to_string()];
        for (i, pos) in session.vim.jump_list.history.iter().enumerate() {
            let p_idx = pos.path.0.first().copied().unwrap_or(0) + 1;
            let snippet = session.doc.para_at(pos).map(|p| p.text.chars().take(20).collect::<String>()).unwrap_or_default();
            let marker = if i == session.vim.jump_list.current_idx { ">" } else { " " };
            lines.push(format!("{marker}{i:>3} {p_idx:>4} {:>4} {snippet}", pos.off));
        }
        if lines.len() == 1 {
            lines.push("  (no jumps recorded)".to_string());
        }
        return ExResult::ok(lines.join("\n"));
    }

    // Change list inspection: :changes
    if cmd == "changes" {
        let mut lines = vec!["change line  col text".to_string()];
        for (i, pos) in session.vim.change_list.history.iter().enumerate() {
            let p_idx = pos.path.0.first().copied().unwrap_or(0) + 1;
            let snippet = session.doc.para_at(pos).map(|p| p.text.chars().take(20).collect::<String>()).unwrap_or_default();
            let marker = if i == session.vim.change_list.current_idx { ">" } else { " " };
            lines.push(format!("{marker}{i:>4} {p_idx:>4} {:>4} {snippet}", pos.off));
        }
        if lines.len() == 1 {
            lines.push("  (no changes recorded)".to_string());
        }
        return ExResult::ok(lines.join("\n"));
    }

    // Settings: :set [opt]
    if cmd.starts_with("set ") || cmd == "set" {
        let opt = cmd.strip_prefix("set ").unwrap_or("").trim();
        return match opt {
            "number" | "nu" => {
                session.vim.config.line_numbers = true;
                ExResult::ok("number")
            }
            "nonumber" | "nonu" => {
                session.vim.config.line_numbers = false;
                ExResult::ok("nonumber")
            }
            "relativenumber" | "rnu" => {
                session.vim.config.relative_numbers = true;
                ExResult::ok("relativenumber")
            }
            "norelativenumber" | "nornu" => {
                session.vim.config.relative_numbers = false;
                ExResult::ok("norelativenumber")
            }
            _ => ExResult::ok(format!("set {opt}")),
        };
    }

    // Open file: :e <path>, :edit <path>
    if cmd.starts_with("e ") || cmd.starts_with("edit ") {
        let path = cmd.strip_prefix("edit ").or_else(|| cmd.strip_prefix("e ")).map(str::trim).unwrap_or("");
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

fn execute_substitute_range(session: &mut Session, cmd: &str) -> ExResult {
    let rest = if let Some(r) = cmd.strip_prefix('s') {
        r
    } else {
        return ExResult::err("Invalid range substitution syntax");
    };

    let mut chars = rest.chars();
    let Some(delim) = chars.next() else {
        return ExResult::err("Missing delimiter for substitute (e.g. :'<,'>s/find/replace/g)");
    };

    let parts: Vec<&str> = rest[delim.len_utf8()..].split(delim).collect();
    if parts.len() < 2 {
        return ExResult::err(format!("Invalid substitution: expected 2 or 3 parts separated by '{delim}'"));
    }

    let pattern = parts[0];
    let replacement = parts[1];
    if pattern.is_empty() {
        return ExResult::err("Empty search pattern");
    }

    let flags = parts.get(2).copied().unwrap_or("");
    let is_case_insensitive = flags.contains('i');

    let (sel_start, sel_end) = session.sel.ordered();
    let start_p = sel_start.path.0.first().copied().unwrap_or(0) as usize;
    let end_p = sel_end.path.0.first().copied().unwrap_or(0) as usize;
    let mut total_count = 0;

    for p_idx in (start_p..=end_p).rev() {
        let pos = wordcraft_doc::Pos::body(p_idx, 0);
        let Some(para) = session.doc.para_at(&pos).cloned() else { continue };

        let lower_bound = if p_idx == start_p { sel_start.off.min(para.text.len()) } else { 0 };
        let upper_bound = if p_idx == end_p { sel_end.off.min(para.text.len()) } else { para.text.len() };
        if lower_bound >= upper_bound {
            continue;
        }

        let slice = match para.text.get(lower_bound..upper_bound) {
            Some(s) => s,
            None => continue,
        };

        let mut matches = Vec::new();
        if is_case_insensitive {
            let p_lower = pattern.to_lowercase();
            let slice_lower = slice.to_lowercase();
            let mut search_from = 0;
            while let Some(idx) = slice_lower[search_from..].find(&p_lower) {
                let actual_idx = search_from + idx;
                matches.push((lower_bound + actual_idx, lower_bound + actual_idx + pattern.len()));
                search_from = actual_idx + pattern.len().max(1);
            }
        } else {
            let mut search_from = 0;
            while let Some(idx) = slice[search_from..].find(pattern) {
                let actual_idx = search_from + idx;
                matches.push((lower_bound + actual_idx, lower_bound + actual_idx + pattern.len()));
                search_from = actual_idx + pattern.len().max(1);
            }
        }

        // Apply replacements in reverse order within paragraph
        for (m_start, m_end) in matches.into_iter().rev() {
            let pa = wordcraft_doc::Pos::body(p_idx, m_start);
            let pb = wordcraft_doc::Pos::body(p_idx, m_end);
            let props = para.props_of_char(m_start).clone();
            if let Ok(at) = session.doc.delete_range(&pa, &pb) {
                let _ = session.doc.insert_text(&at, replacement, &props);
                total_count += 1;
            }
        }
    }

    session.relayout();
    let _ = session.run("select.collapse", &json!({}));
    ExResult::ok(format!("{total_count} substitutions in selection"))
}
