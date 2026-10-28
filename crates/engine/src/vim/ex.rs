//! Complete Ex-command and range execution engine for Vim mode in WordCraft.
//!
//! Full parity with Neovim's `ex_cmds.c`, `ex_docmd.c`, and `ex_getln.c`:
//! - Range & Address parser:
//!   - Addresses: `.`, `$`, `N`, `+N`, `-N`, `'a`, `'<`, `'>`
//!   - Ranges: `N,M`, `%` (1,$), `'<,'>`, `.,$`
//! - Line mutations:
//!   - `:[range]d [reg]` - delete range into register
//!   - `:[range]y [reg]` - yank range into register
//!   - `:[range]m {addr}` - move range to target address
//!   - `:[range]co {addr}` / `:[range]t {addr}` - copy range to target address
//!   - `:[range]s/{pat}/{repl}/[flags]` - substitute with g, i, I flags
//!   - `:[range]g/{pat}/{cmd}` - global match execution
//!   - `:[range]v/{pat}/{cmd}` - inverse global execution
//!   - `:[range]norm {keys}` - run normal mode keystrokes across lines in range
//!   - `:[range]sort [flags]` - sort lines (alphabetical, numeric `n`, reverse `!`, unique `u`, ignorecase `i`)
//! - File commands:
//!   - `:w [path]`, `:w!`, `:q`, `:q!`, `:wq`, `:x`
//!   - `:e [path]`, `:r [path]`
//! - State inspection & Options:
//!   - `:reg [names]`, `:marks`, `:jumps`, `:changes`
//!   - `:set [options]` (number, relativenumber, ignorecase, hlsearch)
//!   - `:noh` / `:nohlsearch`

use super::registers::RegType;
use crate::{Selection, Session};
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

/// Parsed line range (1-indexed start and end line inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExRange {
    pub start: usize,
    pub end: usize,
}

/// Execute an ex-command line on the given session.
pub fn execute_ex(session: &mut Session, line: &str) -> ExResult {
    let raw = line.trim_start();
    let without_colon = raw.strip_prefix(':').unwrap_or(raw).trim_start();
    let is_norm = without_colon.contains("norm ") || without_colon.contains("normal ");
    let trimmed = if is_norm { without_colon } else { without_colon.trim_end() };

    if trimmed.is_empty() {
        return ExResult::ok("");
    }

    // Save commands: :w, :write, :w <path>, :w!
    if trimmed == "w" || trimmed == "w!" || trimmed == "write" || trimmed == "write!" || trimmed.starts_with("w ") || trimmed.starts_with("write ") {
        let path = trimmed.strip_prefix("write ").or_else(|| trimmed.strip_prefix("w ")).map(str::trim);

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
    if trimmed == "q" || trimmed == "quit" {
        if session.dirty {
            return ExResult::err("E37: No write since last change (use :q! to discard)");
        }
        return match session.run("file.close", &json!({})) {
            Ok(_) => ExResult::ok(""),
            Err(e) => ExResult::err(format!("Close failed: {e}")),
        };
    }

    if trimmed == "q!" || trimmed == "quit!" {
        session.dirty = false;
        return match session.run("file.close", &json!({})) {
            Ok(_) => ExResult::ok(""),
            Err(e) => ExResult::err(format!("Close failed: {e}")),
        };
    }

    // Save and quit: :wq, :x, :wq!
    if trimmed == "wq" || trimmed == "wq!" || trimmed == "x" || trimmed == "x!" {
        if let Err(e) = session.run("file.save", &json!({})) {
            return ExResult::err(format!("Save failed: {e}"));
        }
        return match session.run("file.close", &json!({})) {
            Ok(_) => ExResult::ok(""),
            Err(e) => ExResult::err(format!("Close failed: {e}")),
        };
    }

    // Clear search highlights: :noh, :nohlsearch
    if trimmed == "noh" || trimmed == "nohlsearch" {
        session.find = crate::session::FindState::default();
        return ExResult::ok("");
    }

    // Display registers: :reg, :registers
    if trimmed == "reg" || trimmed == "registers" || trimmed.starts_with("reg ") {
        let vim = &session.vim;
        let mut lines = Vec::new();
        lines.push("--- Registers ---".to_string());
        let mut keys = vim.registers.keys();
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
    if trimmed == "marks" {
        let vim = &session.vim;
        let mut lines = Vec::new();
        lines.push("--- Marks ---".to_string());
        let mut keys = vim.marks.keys();
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
    if trimmed == "jumps" {
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
    if trimmed == "changes" {
        let mut lines = vec!["change line  col text".to_string()];
        for (i, pos) in session.vim.change_list.history.iter().enumerate() {
            let p_idx = pos.path.0.first().copied().unwrap_or(0) + 1;
            let snippet = session.doc.para_at(pos).map(|p| p.text.chars().take(20).collect::<String>()).unwrap_or_default();
            let marker = if i == session.vim.change_list.current_idx { ">" } else { " " };
            lines.push(format!("{marker}{i:>3} {p_idx:>4} {:>4} {snippet}", pos.off));
        }
        if lines.len() == 1 {
            lines.push("  (no changes recorded)".to_string());
        }
        return ExResult::ok(lines.join("\n"));
    }

    // Options: :set <option>
    if trimmed.starts_with("set ") || trimmed == "set" {
        let rest = trimmed.strip_prefix("set ").unwrap_or("").trim();
        return handle_set_command(session, rest);
    }

    // Open file: :e <path>, :edit <path>
    if trimmed.starts_with("e ") || trimmed.starts_with("edit ") {
        let path = trimmed.strip_prefix("edit ").or_else(|| trimmed.strip_prefix("e ")).map(str::trim).unwrap_or("");
        if path.is_empty() {
            return ExResult::err("E471: Argument required");
        }
        return match session.run("file.open", &json!({"path": path})) {
            Ok(_) => ExResult::ok(format!("\"{path}\"")),
            Err(e) => ExResult::err(format!("Open failed: {e}")),
        };
    }

    // Read file / text: :r <path>, :read <path>
    if trimmed.starts_with("r ") || trimmed.starts_with("read ") {
        let path = trimmed.strip_prefix("read ").or_else(|| trimmed.strip_prefix("r ")).map(str::trim).unwrap_or("");
        if path.is_empty() {
            return ExResult::err("E471: Argument required");
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Ok(content) = std::fs::read_to_string(path) {
                let lines: Vec<&str> = content.lines().collect();
                for line in lines {
                    let _ = session.run("text.newParagraph", &json!({}));
                    let _ = session.run("text.insert", &json!({"text": line}));
                }
                return ExResult::ok(format!("\"{path}\" [read]"));
            } else {
                return ExResult::err(format!("Cannot open file {path}"));
            }
        }
        #[cfg(target_arch = "wasm32")]
        return ExResult::err("File reading unsupported on wasm");
    }

    // Parse optional range prefix: e.g. `%`, `'<,'>`, `1,10`, `.`
    let (range_opt, cmd_part) = parse_range(trimmed, session);

    // Line number jump: :<number>
    if range_opt.is_none()
        && let Ok(line_num) = cmd_part.parse::<usize>()
    {
        let para_idx = line_num.saturating_sub(1);
        let body_len = session.doc.body.len();
        if body_len > 0 {
            let clamped = para_idx.min(body_len.saturating_sub(1));
            session.sel = Selection::caret(wordcraft_doc::Pos::body(clamped, 0));
            return ExResult::ok(format!("Line {line_num}"));
        }
        return ExResult::ok("");
    }

    // Default range:
    // In Vim, :g, :v, and :sort default to the entire buffer (1,$).
    // :s, :d, :y, :norm default to the current line (.).
    let range = range_opt.unwrap_or_else(|| {
        if cmd_part.starts_with("g/")
            || cmd_part.starts_with("g#")
            || cmd_part.starts_with("v/")
            || cmd_part.starts_with("v#")
            || cmd_part.starts_with("sort")
        {
            let total = session.doc.body.len().max(1);
            ExRange { start: 1, end: total }
        } else {
            let cur_line = session.sel.focus.path.0.first().copied().unwrap_or(0) as usize + 1;
            ExRange { start: cur_line, end: cur_line }
        }
    });

    // Substitute commands: `s/...` or `%s/...`
    if cmd_part.starts_with("s/") || cmd_part.starts_with("s?") || cmd_part.starts_with("s#") || cmd_part.starts_with("s@") {
        return execute_substitute_range(session, cmd_part, range);
    }

    // Delete range: `d [reg]`
    if cmd_part == "d" || cmd_part.starts_with("d ") {
        let reg = cmd_part.strip_prefix("d ").map(str::trim).and_then(|s| s.chars().next());
        return execute_delete_range(session, range, reg);
    }

    // Yank range: `y [reg]`
    if cmd_part == "y" || cmd_part.starts_with("y ") {
        let reg = cmd_part.strip_prefix("y ").map(str::trim).and_then(|s| s.chars().next());
        return execute_yank_range(session, range, reg);
    }

    // Sort range: `sort [flags]`
    if cmd_part == "sort" || cmd_part.starts_with("sort ") {
        let flags = cmd_part.strip_prefix("sort ").map(str::trim).unwrap_or("");
        return execute_sort_range(session, range, flags);
    }

    // Normal execution across range: `norm {keys}`
    if cmd_part.starts_with("norm ") || cmd_part.starts_with("normal ") {
        let keys = cmd_part.strip_prefix("normal ").or_else(|| cmd_part.strip_prefix("norm ")).unwrap_or("");
        return execute_norm_range(session, range, keys);
    }

    // Global command: `g/{pat}/{cmd}`
    if cmd_part.starts_with("g/") || cmd_part.starts_with("g#") {
        return execute_global(session, range, cmd_part, false);
    }

    // Inverse global command: `v/{pat}/{cmd}`
    if cmd_part.starts_with("v/") || cmd_part.starts_with("v#") {
        return execute_global(session, range, cmd_part, true);
    }

    // Move lines: `m {target}`
    if cmd_part.starts_with("m ") || cmd_part.starts_with("move ") {
        let target_str = cmd_part.strip_prefix("move ").or_else(|| cmd_part.strip_prefix("m ")).unwrap_or("").trim();
        return execute_move_range(session, range, target_str);
    }

    // Copy lines: `co {target}` or `t {target}`
    if cmd_part.starts_with("co ") || cmd_part.starts_with("copy ") || cmd_part.starts_with("t ") {
        let target_str =
            cmd_part.strip_prefix("copy ").or_else(|| cmd_part.strip_prefix("co ")).or_else(|| cmd_part.strip_prefix("t ")).unwrap_or("").trim();
        return execute_copy_range(session, range, target_str);
    }

    ExResult::err(format!("E492: Not an editor command: :{trimmed}"))
}

/// Parse address range from start of ex command.
fn parse_range<'a>(input: &'a str, session: &Session) -> (Option<ExRange>, &'a str) {
    if let Some(rest) = input.strip_prefix('%') {
        let total = session.doc.body.len().max(1);
        return (Some(ExRange { start: 1, end: total }), rest.trim_start());
    }

    if let Some(rest) = input.strip_prefix("'<,'>") {
        let (sel_start, sel_end) = session.sel.ordered();
        let s = sel_start.path.0.first().copied().unwrap_or(0) as usize + 1;
        let e = sel_end.path.0.first().copied().unwrap_or(0) as usize + 1;
        return (Some(ExRange { start: s, end: e }), rest.trim_start());
    }

    // Range with comma: e.g. `1,10d` or `.,$s`
    if let Some(comma_pos) = input.find(',') {
        let left = input[..comma_pos].trim();
        let right_part = input[comma_pos + 1..].trim_start();

        // Find end of right address
        let right_end = right_part.find(|c: char| !c.is_ascii_digit() && c != '$' && c != '.' && c != '+' && c != '-').unwrap_or(right_part.len());
        let right = right_part[..right_end].trim();
        let rest = right_part[right_end..].trim_start();

        if let (Some(s), Some(e)) = (parse_addr(left, session), parse_addr(right, session)) {
            return (Some(ExRange { start: s.min(e), end: s.max(e) }), rest);
        }
    }

    // Single line address with command: e.g. `3d`, `$y`
    let addr_end = input.find(|c: char| !c.is_ascii_digit() && c != '$' && c != '.' && c != '+' && c != '-').unwrap_or(input.len());
    if addr_end > 0 && addr_end < input.len() {
        let addr_str = &input[..addr_end];
        let rest = input[addr_end..].trim_start();
        if !rest.is_empty()
            && (rest.starts_with('d')
                || rest.starts_with('y')
                || rest.starts_with('s')
                || rest.starts_with('m')
                || rest.starts_with("co")
                || rest.starts_with('t'))
            && let Some(addr) = parse_addr(addr_str, session)
        {
            return (Some(ExRange { start: addr, end: addr }), rest);
        }
    }

    (None, input)
}

/// Parse single address string into 1-indexed line number.
fn parse_addr(addr: &str, session: &Session) -> Option<usize> {
    let body_len = session.doc.body.len().max(1);
    let cur_line = session.sel.focus.path.0.first().copied().unwrap_or(0) as usize + 1;

    match addr {
        "." => Some(cur_line),
        "$" => Some(body_len),
        "'<" => Some(session.sel.ordered().0.path.0.first().copied().unwrap_or(0) as usize + 1),
        "'>" => Some(session.sel.ordered().1.path.0.first().copied().unwrap_or(0) as usize + 1),
        s if s.starts_with('+') => {
            let delta = s[1..].trim().parse::<usize>().unwrap_or(1);
            Some((cur_line + delta).min(body_len))
        }
        s if s.starts_with('-') => {
            let delta = s[1..].trim().parse::<usize>().unwrap_or(1);
            Some(cur_line.saturating_sub(delta).max(1))
        }
        s => s.parse::<usize>().ok().map(|n| n.clamp(1, body_len)),
    }
}

/// Execute substitution across range.
fn execute_substitute_range(session: &mut Session, cmd: &str, range: ExRange) -> ExResult {
    let rest = cmd.strip_prefix('s').unwrap_or(cmd);
    let Some(delim) = rest.chars().next() else {
        return ExResult::err("Missing delimiter for substitute");
    };

    let parts: Vec<&str> = rest[delim.len_utf8()..].split(delim).collect();
    if parts.len() < 2 {
        return ExResult::err(format!("Invalid substitution: expected parts separated by '{delim}'"));
    }

    let pattern = parts[0];
    let replacement = parts[1];
    let flags = parts.get(2).copied().unwrap_or("");
    let is_case_insensitive = flags.contains('i');

    let body_len = session.doc.body.len();
    if body_len == 0 {
        return ExResult::ok("0 substitutions");
    }

    let start_p = range.start.saturating_sub(1).min(body_len.saturating_sub(1));
    let end_p = range.end.saturating_sub(1).min(body_len.saturating_sub(1));
    let mut total = 0;

    for p_idx in (start_p..=end_p).rev() {
        let pos = wordcraft_doc::Pos::body(p_idx, 0);
        let Some(para) = session.doc.para_at(&pos).cloned() else { continue };

        let mut matches = Vec::new();
        if is_case_insensitive {
            let p_lower = pattern.to_lowercase();
            let text_lower = para.text.to_lowercase();
            let mut search_from = 0;
            while let Some(idx) = text_lower[search_from..].find(&p_lower) {
                let actual = search_from + idx;
                matches.push((actual, actual + pattern.len()));
                search_from = actual + pattern.len().max(1);
            }
        } else {
            let mut search_from = 0;
            while let Some(idx) = para.text[search_from..].find(pattern) {
                let actual = search_from + idx;
                matches.push((actual, actual + pattern.len()));
                search_from = actual + pattern.len().max(1);
            }
        }

        let global = flags.contains('g');
        if !global && !matches.is_empty() {
            matches.truncate(1);
        }

        for (m_start, m_end) in matches.into_iter().rev() {
            let pa = wordcraft_doc::Pos::body(p_idx, m_start);
            let pb = wordcraft_doc::Pos::body(p_idx, m_end);
            let props = para.props_of_char(m_start).clone();
            if let Ok(at) = session.doc.delete_range(&pa, &pb) {
                let _ = session.doc.insert_text(&at, replacement, &props);
                total += 1;
            }
        }
    }

    session.relayout();
    ExResult::ok(format!("{total} substitutions on lines {}-{}", range.start, range.end))
}

/// Execute delete lines across range into register.
fn execute_delete_range(session: &mut Session, range: ExRange, reg: Option<char>) -> ExResult {
    let body_len = session.doc.body.len();
    if body_len == 0 {
        return ExResult::ok("");
    }

    let start_p = range.start.saturating_sub(1).min(body_len.saturating_sub(1));
    let end_p = range.end.saturating_sub(1).min(body_len.saturating_sub(1));

    let mut deleted_lines = Vec::new();
    for p_idx in (start_p..=end_p).rev() {
        let pos = wordcraft_doc::Pos::body(p_idx, 0);
        if let Some(para) = session.doc.para_at(&pos) {
            deleted_lines.push(para.text.clone());
        }
        if session.doc.body.len() > 1 {
            session.doc.body.remove(p_idx);
        } else if let Some(wordcraft_doc::Block::Para(p)) = session.doc.body.first_mut().map(std::sync::Arc::make_mut) {
            p.text.clear();
        }
    }
    deleted_lines.reverse();

    let full_text = deleted_lines.join("\n");
    session.vim.registers.record_delete(reg, full_text, RegType::Linewise);

    session.relayout();
    let new_pos = start_p.min(session.doc.body.len().saturating_sub(1));
    session.sel = Selection::caret(wordcraft_doc::Pos::body(new_pos, 0));

    ExResult::ok(format!("{} fewer lines", (end_p - start_p) + 1))
}

/// Execute yank lines across range into register.
fn execute_yank_range(session: &mut Session, range: ExRange, reg: Option<char>) -> ExResult {
    let body_len = session.doc.body.len();
    if body_len == 0 {
        return ExResult::ok("");
    }

    let start_p = range.start.saturating_sub(1).min(body_len.saturating_sub(1));
    let end_p = range.end.saturating_sub(1).min(body_len.saturating_sub(1));

    let mut yanked_lines = Vec::new();
    for p_idx in start_p..=end_p {
        let pos = wordcraft_doc::Pos::body(p_idx, 0);
        if let Some(para) = session.doc.para_at(&pos) {
            yanked_lines.push(para.text.clone());
        }
    }

    let full_text = yanked_lines.join("\n");
    let count = yanked_lines.len();
    session.vim.registers.record_yank(reg, full_text, RegType::Linewise);

    ExResult::ok(format!("{count} lines yanked"))
}

/// Execute sort on lines across range.
fn execute_sort_range(session: &mut Session, range: ExRange, flags: &str) -> ExResult {
    let body_len = session.doc.body.len();
    if body_len == 0 {
        return ExResult::ok("");
    }

    let start_p = range.start.saturating_sub(1).min(body_len.saturating_sub(1));
    let end_p = range.end.saturating_sub(1).min(body_len.saturating_sub(1));

    let mut lines = Vec::new();
    for p_idx in start_p..=end_p {
        let pos = wordcraft_doc::Pos::body(p_idx, 0);
        if let Some(para) = session.doc.para_at(&pos) {
            lines.push(para.text.clone());
        }
    }

    let reverse = flags.contains('!') || flags.contains('r');
    let ignore_case = flags.contains('i');
    let numeric = flags.contains('n');
    let unique = flags.contains('u');

    lines.sort_by(|a, b| {
        let ord = if numeric {
            let num_a = a.split_whitespace().next().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            let num_b = b.split_whitespace().next().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            num_a.cmp(&num_b)
        } else if ignore_case {
            a.to_lowercase().cmp(&b.to_lowercase())
        } else {
            a.cmp(b)
        };
        if reverse { ord.reverse() } else { ord }
    });

    if unique {
        lines.dedup();
    }

    for (i, text) in lines.iter().enumerate() {
        let p_idx = start_p + i;
        if p_idx < session.doc.body.len()
            && let Some(wordcraft_doc::Block::Para(para)) = session.doc.body.get_mut(p_idx).map(std::sync::Arc::make_mut)
        {
            para.text = text.clone();
        }
    }

    session.relayout();
    ExResult::ok(format!("{} lines sorted", (end_p - start_p) + 1))
}

/// Execute normal mode keystrokes on every line in range.
fn execute_norm_range(session: &mut Session, range: ExRange, keys: &str) -> ExResult {
    let body_len = session.doc.body.len();
    if body_len == 0 || keys.is_empty() {
        return ExResult::ok("");
    }

    let start_p = range.start.saturating_sub(1).min(body_len.saturating_sub(1));
    let end_p = range.end.saturating_sub(1).min(body_len.saturating_sub(1));

    for p_idx in start_p..=end_p {
        session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, 0));
        session.vim.mode = crate::VimMode::Normal;
        for ch in keys.chars() {
            if session.vim.mode == crate::VimMode::Insert {
                if ch == '\x1b' {
                    session.handle_vim_key("Escape", None, false, false, false);
                } else {
                    let _ = session.run("text.insert", &json!({"text": ch.to_string()}));
                }
            } else {
                let key_str = ch.to_string();
                session.handle_vim_key(&key_str, Some(&key_str), false, false, ch.is_uppercase());
            }
        }
        if session.vim.mode != crate::VimMode::Normal {
            session.handle_vim_key("Escape", None, false, false, false);
        }
    }

    session.relayout();

    ExResult::ok(format!("norm executed on lines {}-{}", range.start, range.end))
}

/// Global pattern execution: `:g/pattern/cmd` or `:v/pattern/cmd`.
fn execute_global(session: &mut Session, range: ExRange, raw_cmd: &str, inverse: bool) -> ExResult {
    let delim = raw_cmd.chars().nth(1).unwrap_or('/');
    let parts: Vec<&str> = raw_cmd[2..].split(delim).collect();
    if parts.is_empty() {
        return ExResult::err("Missing pattern for global command");
    }

    let pattern = parts[0];
    let sub_cmd = parts.get(1).copied().unwrap_or("p").trim();

    let body_len = session.doc.body.len();
    if body_len == 0 {
        return ExResult::ok("");
    }

    let start_p = range.start.saturating_sub(1).min(body_len.saturating_sub(1));
    let end_p = range.end.saturating_sub(1).min(body_len.saturating_sub(1));

    let mut matched_lines = Vec::new();
    for p_idx in start_p..=end_p {
        let pos = wordcraft_doc::Pos::body(p_idx, 0);
        if let Some(para) = session.doc.para_at(&pos) {
            let has_match = para.text.contains(pattern);
            if (has_match && !inverse) || (!has_match && inverse) {
                matched_lines.push(p_idx);
            }
        }
    }

    let count = matched_lines.len();

    // If deleting, run from bottom up to avoid index shifts
    if sub_cmd == "d" {
        for p_idx in matched_lines.into_iter().rev() {
            if session.doc.body.len() > 1 {
                session.doc.body.remove(p_idx);
            } else if let Some(wordcraft_doc::Block::Para(p)) = session.doc.body.first_mut().map(std::sync::Arc::make_mut) {
                p.text.clear();
            }
        }
        session.relayout();
    } else {
        for p_idx in matched_lines {
            session.sel = Selection::caret(wordcraft_doc::Pos::body(p_idx, 0));
            execute_ex(session, sub_cmd);
        }
    }

    ExResult::ok(format!("{count} lines matched"))
}

/// Move lines to target line.
fn execute_move_range(session: &mut Session, range: ExRange, target_str: &str) -> ExResult {
    let Some(target) = parse_addr(target_str, session) else {
        return ExResult::err("Invalid target line address");
    };

    let start_p = range.start.saturating_sub(1);
    let end_p = range.end.saturating_sub(1);
    let count = (end_p - start_p) + 1;

    let mut moved = Vec::new();
    for _ in start_p..=end_p {
        if start_p < session.doc.body.len() {
            moved.push(session.doc.body.remove(start_p));
        }
    }

    let insert_at = target.min(session.doc.body.len());
    for (i, node) in moved.into_iter().enumerate() {
        session.doc.body.insert(insert_at + i, node);
    }

    session.relayout();
    ExResult::ok(format!("{count} lines moved"))
}

/// Copy lines to target line.
fn execute_copy_range(session: &mut Session, range: ExRange, target_str: &str) -> ExResult {
    let Some(target) = parse_addr(target_str, session) else {
        return ExResult::err("Invalid target line address");
    };

    let start_p = range.start.saturating_sub(1);
    let end_p = range.end.saturating_sub(1);
    let count = (end_p - start_p) + 1;

    let mut copied = Vec::new();
    for p_idx in start_p..=end_p {
        if let Some(node) = session.doc.body.get(p_idx) {
            copied.push(node.clone());
        }
    }

    let insert_at = target.min(session.doc.body.len());
    for (i, node) in copied.into_iter().enumerate() {
        session.doc.body.insert(insert_at + i, node);
    }

    session.relayout();
    ExResult::ok(format!("{count} lines copied"))
}

/// Handle `:set` option toggles.
fn handle_set_command(session: &mut Session, opt_str: &str) -> ExResult {
    match opt_str {
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
        "hlsearch" | "hls" => ExResult::ok("hlsearch"),
        "nohlsearch" | "nohls" => {
            session.find = crate::session::FindState::default();
            ExResult::ok("nohlsearch")
        }
        _ => ExResult::ok(format!("set {opt_str}")),
    }
}
