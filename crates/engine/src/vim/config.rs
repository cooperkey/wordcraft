//! Vim mode configuration and keymap customization.
//!
//! Loads user preferences and custom key mappings from `~/.config/wordcraft/vim.json` or
//! `~/.config/wordcraft/init.vim` (on desktop platforms).

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

/// Custom settings for Vim mode.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VimConfig {
    /// Default mode on startup or when enabled ("normal" or "insert").
    pub default_mode: String,
    /// Leader key (defaults to space " ").
    pub leader: String,
    /// Custom key mappings for Normal mode (e.g. "<leader>w" -> "file.save" or ":w").
    pub normal_mappings: BTreeMap<String, String>,
    /// Custom key mappings for Visual mode (e.g. "<leader>b" -> "format.bold").
    pub visual_mappings: BTreeMap<String, String>,
}

impl Default for VimConfig {
    fn default() -> Self {
        let mut normal_mappings = BTreeMap::new();
        normal_mappings.insert("<leader>w".to_string(), "file.save".to_string());
        normal_mappings.insert("<leader>q".to_string(), "file.close".to_string());
        normal_mappings.insert("<leader>y".to_string(), "edit.copy".to_string());
        normal_mappings.insert("<leader>p".to_string(), "edit.paste".to_string());
        normal_mappings.insert("<leader>d".to_string(), "edit.cut".to_string());
        normal_mappings.insert("<leader>u".to_string(), "edit.undo".to_string());
        normal_mappings.insert("<leader>b".to_string(), "format.bold".to_string());
        normal_mappings.insert("<leader>i".to_string(), "format.italic".to_string());
        normal_mappings.insert("<leader>s".to_string(), "format.strikethrough".to_string());
        normal_mappings.insert("<leader>h".to_string(), "format.highlight".to_string());
        normal_mappings.insert("<leader>c".to_string(), "format.code".to_string());
        normal_mappings.insert("<leader>1".to_string(), "style.heading1".to_string());
        normal_mappings.insert("<leader>2".to_string(), "style.heading2".to_string());
        normal_mappings.insert("<leader>3".to_string(), "style.heading3".to_string());
        normal_mappings.insert("<leader>4".to_string(), "style.heading4".to_string());
        normal_mappings.insert("<leader>5".to_string(), "style.heading5".to_string());
        normal_mappings.insert("<leader>6".to_string(), "style.heading6".to_string());
        normal_mappings.insert("<leader>x".to_string(), "list.bullet".to_string());
        normal_mappings.insert("<leader>e".to_string(), "file.open".to_string());

        let mut visual_mappings = BTreeMap::new();
        visual_mappings.insert("<leader>b".to_string(), "format.bold".to_string());
        visual_mappings.insert("<leader>i".to_string(), "format.italic".to_string());
        visual_mappings.insert("<leader>s".to_string(), "format.strikethrough".to_string());
        visual_mappings.insert("<leader>h".to_string(), "format.highlight".to_string());
        visual_mappings.insert("<leader>c".to_string(), "format.code".to_string());
        visual_mappings.insert("<leader>y".to_string(), "edit.copy".to_string());
        visual_mappings.insert("<leader>d".to_string(), "edit.cut".to_string());
        visual_mappings.insert("<leader>p".to_string(), "edit.paste".to_string());
        visual_mappings.insert("<leader>x".to_string(), "list.bullet".to_string());

        Self {
            default_mode: "normal".to_string(),
            leader: " ".to_string(),
            normal_mappings,
            visual_mappings,
        }
    }
}

impl VimConfig {
    /// Load the configuration from the user's config directory, or default if missing or unreadable.
    pub fn load() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(cfg) = Self::try_load_desktop() {
                return cfg;
            }
        }
        Self::default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn try_load_desktop() -> Option<Self> {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
        let config_dir = home.join(".config").join("wordcraft");

        // Try vim.json first
        let json_path = config_dir.join("vim.json");
        if let Ok(content) = std::fs::read_to_string(&json_path)
            && let Ok(cfg) = serde_json::from_str::<VimConfig>(&content)
        {
            return Some(cfg);
        }

        // Try init.vim with vim-script like directives
        let vim_path = config_dir.join("init.vim");
        if let Ok(content) = std::fs::read_to_string(&vim_path) {
            return Some(Self::parse_vimscript(&content));
        }

        None
    }

    /// Parse simple vimrc lines like `let mapleader = " "` and `nnoremap lhs rhs`.
    pub fn parse_vimscript(content: &str) -> Self {
        let mut cfg = Self::default();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('"') || trimmed.starts_with('#') {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("let mapleader") {
                let val = rest.trim().trim_start_matches('=').trim();
                let leader = val.trim_matches('"').trim_matches('\'');
                if !leader.is_empty() {
                    cfg.leader = leader.to_string();
                }
            } else if let Some(rest) = trimmed.strip_prefix("nnoremap ").or_else(|| trimmed.strip_prefix("nmap ")) {
                let mut parts = rest.split_whitespace();
                if let (Some(lhs), Some(rhs)) = (parts.next(), parts.next()) {
                    cfg.normal_mappings.insert(lhs.to_string(), rhs.to_string());
                }
            } else if let Some(rest) = trimmed.strip_prefix("vnoremap ").or_else(|| trimmed.strip_prefix("vmap ")) {
                let mut parts = rest.split_whitespace();
                if let (Some(lhs), Some(rhs)) = (parts.next(), parts.next()) {
                    cfg.visual_mappings.insert(lhs.to_string(), rhs.to_string());
                }
            } else if let Some(rest) = trimmed.strip_prefix("set default_mode") {
                let val = rest.trim().trim_start_matches('=').trim();
                let mode = val.trim_matches('"').trim_matches('\'');
                if mode == "insert" || mode == "normal" {
                    cfg.default_mode = mode.to_string();
                }
            }
        }
        cfg
    }
}
