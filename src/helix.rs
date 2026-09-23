//! Helix-style modal input state. Selections and text edits live in `Editor`.

use crate::editor::HelixSelectionSet;
use crossterm::event::KeyEvent;
use std::collections::HashMap;

const JUMP_LIST_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HelixMode {
    #[default]
    Normal,
    Insert,
    Select,
    Command,
    SearchForward,
    SearchBackward,
    RegexSelect,
    RegexSplit,
    RegexKeep(bool),
}

impl HelixMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Select => "SELECT",
            Self::Command => "COMMAND",
            Self::SearchForward | Self::SearchBackward => "SEARCH",
            Self::RegexSelect | Self::RegexSplit | Self::RegexKeep(_) => "REGEX",
        }
    }

    pub fn is_prompt(self) -> bool {
        !matches!(self, Self::Normal | Self::Insert | Self::Select)
    }
}

#[derive(Debug, Default)]
pub struct HelixState {
    pub mode: HelixMode,
    pub pending: Option<char>,
    pub pending_count: usize,
    pub prompt: String,
    pub count: String,
    pub registers: HashMap<char, Vec<String>>,
    pub selected_register: Option<char>,
    pub search_pattern: String,
    pub last_insert: String,
    pub last_find: Option<(char, bool, bool)>,
    pub surround_from: Option<char>,
    pub jump_list: Vec<HelixSelectionSet>,
    pub jump_index: usize,
    pub status_message: Option<String>,
    pub macros: HashMap<char, Vec<KeyEvent>>,
    pub recording: Option<char>,
    pub last_macro: Option<char>,
    pub playing_macro: bool,
    pub sticky_view: bool,
    pub restore_cursor: bool,
}

impl HelixState {
    pub fn reset_transient(&mut self) {
        self.mode = HelixMode::Normal;
        self.pending = None;
        self.pending_count = 1;
        self.prompt.clear();
        self.count.clear();
        self.selected_register = None;
        self.surround_from = None;
        self.status_message = None;
        self.recording = None;
        self.playing_macro = false;
        self.sticky_view = false;
        self.restore_cursor = false;
    }

    pub fn take_count(&mut self) -> usize {
        let count = self.count.parse().unwrap_or(1);
        self.count.clear();
        count.clamp(1, 10_000)
    }

    pub fn take_register(&mut self) -> char {
        self.selected_register.take().unwrap_or('"')
    }

    pub fn push_jump(&mut self, jump: HelixSelectionSet) {
        self.jump_list.truncate(self.jump_index);
        if self.jump_list.last() != Some(&jump) {
            self.jump_list.push(jump);
        }
        if self.jump_list.len() > JUMP_LIST_LIMIT {
            self.jump_list.remove(0);
        }
        self.jump_index = self.jump_list.len();
    }

    pub fn jump_backward(&mut self, current: HelixSelectionSet, count: usize) -> Option<HelixSelectionSet> {
        let target = self.jump_index.checked_sub(count)?;
        if self.jump_index == self.jump_list.len() {
            self.push_jump(current);
        }
        self.jump_index = target.min(self.jump_list.len().saturating_sub(1));
        self.jump_list.get(self.jump_index).cloned()
    }

    pub fn jump_forward(&mut self, count: usize) -> Option<HelixSelectionSet> {
        let target = self.jump_index + count;
        if target >= self.jump_list.len() {
            return None;
        }
        self.jump_index = target;
        self.jump_list.get(target).cloned()
    }

    pub fn pending_label(&self) -> String {
        let pending = match self.pending {
            Some('A') => "ma".to_string(),
            Some('I') => "mi".to_string(),
            Some('s') => "ms".to_string(),
            Some('d') => "md".to_string(),
            Some('1') => "mr".to_string(),
            Some('2') => format!("mr{}", self.surround_from.unwrap_or(' ')),
            Some('R') => "C-r".to_string(),
            Some(pending) => pending.to_string(),
            None if self.sticky_view => "Z".to_string(),
            None => String::new(),
        };
        let register = self.selected_register.map_or_else(String::new, |register| format!("\"{register}"));
        format!("{register}{}{pending}", self.count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::HelixSelection;

    fn jump(offset: usize) -> HelixSelectionSet {
        HelixSelectionSet { selections: vec![HelixSelection::caret(offset)], primary: 0 }
    }

    #[test]
    fn jump_list_returns_to_the_latest_position() {
        let mut state = HelixState::default();
        state.push_jump(jump(1));
        state.push_jump(jump(5));
        assert_eq!(state.jump_backward(jump(9), 1), Some(jump(5)));
        assert_eq!(state.jump_backward(jump(5), 1), Some(jump(1)));
        assert_eq!(state.jump_backward(jump(1), 1), None);
        assert_eq!(state.jump_forward(1), Some(jump(5)));
        assert_eq!(state.jump_forward(1), Some(jump(9)));
        assert_eq!(state.jump_forward(1), None);
    }
}
