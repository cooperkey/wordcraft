//! Authentic flash.nvim jump engine for WordCraft.
//! Implements 2-char / 1-char search with backdrop dimming, distance-prioritized
//! home-row jump labels, overlay badges, and instant jumping.

use crate::{Selection, Session};
use wordcraft_doc::Pos;

/// Standard home-row prioritized label pool matching flash.nvim default configuration.
pub const FLASH_LABELS: &[char] = &[
    'a', 's', 'd', 'f', 'j', 'k', 'l', 'g', 'h', 'w', 'e', 'r', 'u', 'i', 'o', 'p', 'z', 'x', 'c', 'v', 'b', 'n', 'm', 'A', 'S', 'D', 'F', 'J', 'K',
    'L', 'G', 'H', 'W', 'E', 'R', 'U', 'I', 'O', 'P',
];

/// A single flash jump target on screen.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FlashTarget {
    pub pos: Pos,
    pub label: char,
}

/// Active flash.nvim state during an interactive jump session.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FlashState {
    /// Active query string typed so far (e.g. "w" or "wo").
    pub query: String,
    /// Active jump targets with their assigned labels.
    pub targets: Vec<FlashTarget>,
    /// Whether flash is active.
    pub active: bool,
    /// If triggered by f/t/F/T (motion mode).
    pub motion_mode: Option<char>,
}

impl FlashState {
    /// Start a new flash jump session.
    pub fn start(&mut self) {
        self.query.clear();
        self.targets.clear();
        self.active = true;
        self.motion_mode = None;
    }

    /// Start an enhanced character motion session (f/F/t/T).
    pub fn start_motion(&mut self, motion_char: char) {
        self.query.clear();
        self.targets.clear();
        self.active = true;
        self.motion_mode = Some(motion_char);
    }

    /// Cancel the flash session.
    pub fn cancel(&mut self) {
        self.query.clear();
        self.targets.clear();
        self.active = false;
        self.motion_mode = None;
    }

    /// Find a target by its label.
    pub fn target_for_label(&self, label: char) -> Option<Pos> {
        self.targets.iter().find(|t| t.label == label).map(|t| t.pos.clone())
    }
}

/// Update flash targets based on current query and cursor position.
/// Returns true if a single target was auto-jumped.
pub fn update_flash_targets(session: &mut Session, flash: &mut FlashState) -> bool {
    flash.targets.clear();
    if flash.query.is_empty() {
        return false;
    }

    let focus = session.sel.focus.clone();
    let current_p_idx = focus.path.0.first().copied().unwrap_or(0) as usize;
    let query_lower = flash.query.to_lowercase();
    let body_len = session.doc.body.len();

    // Collect all matches across the document (starting around the cursor)
    let mut raw_targets: Vec<(Pos, usize)> = Vec::new();

    // Search window of paragraphs around cursor first
    let start_p = current_p_idx.saturating_sub(30);
    let end_p = (current_p_idx.saturating_add(30)).min(body_len);

    for p_idx in start_p..end_p {
        let pos = Pos::body(p_idx, 0);
        if let Some(para) = session.doc.para_at(&pos) {
            let txt = para.text.to_lowercase();
            let mut search_start = 0;
            while let Some(idx) = txt.get(search_start..).and_then(|s| s.find(&query_lower)) {
                let match_off = search_start.saturating_add(idx);
                let target_pos = Pos::body(p_idx, match_off);
                // Compute distance from cursor
                let p_dist = (p_idx as isize - current_p_idx as isize).unsigned_abs();
                let off_dist = (match_off as isize - focus.off as isize).unsigned_abs();
                let dist = p_dist * 1000 + off_dist;
                raw_targets.push((target_pos, dist));
                search_start = match_off.saturating_add(query_lower.len().max(1));
            }
        }
    }

    if raw_targets.is_empty() {
        return false;
    }

    // Sort targets by proximity to cursor
    raw_targets.sort_by_key(|(_, dist)| *dist);

    // If exactly one match and query length >= 2: autojump immediately
    if raw_targets.len() == 1 && flash.query.len() >= 2 {
        let (pos, _) = &raw_targets[0];
        session.sel = Selection::caret(pos.clone());
        flash.cancel();
        return true;
    }

    // Assign labels from the pool
    for (i, (pos, _)) in raw_targets.into_iter().enumerate() {
        if let Some(&label) = FLASH_LABELS.get(i) {
            flash.targets.push(FlashTarget { pos, label });
        } else {
            break;
        }
    }

    false
}
