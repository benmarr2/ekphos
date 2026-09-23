use super::*;
use std::cmp::Ordering;

/// Character offsets include newlines. A Helix cursor is a one-character selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelixSelection {
    pub anchor: usize,
    pub head: usize,
}

impl HelixSelection {
    pub fn caret(offset: usize) -> Self {
        Self { anchor: offset, head: offset }
    }

    pub fn spanning(start: usize, end: usize, reversed: bool) -> Self {
        if end <= start {
            return Self::caret(start);
        }
        if reversed {
            Self { anchor: end - 1, head: start }
        } else {
            Self { anchor: start, head: end - 1 }
        }
    }

    pub fn range(self, text_len: usize) -> (usize, usize) {
        (self.anchor.min(self.head).min(text_len), self.anchor.max(self.head).saturating_add(1).min(text_len))
    }

    pub fn is_reversed(self) -> bool {
        self.head < self.anchor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelixSelectionSet {
    pub selections: Vec<HelixSelection>,
    pub primary: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelixChange {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub selection: Option<(usize, usize)>,
}

impl HelixChange {
    pub fn new(start: usize, end: usize, text: impl Into<String>) -> Self {
        Self { start, end, text: text.into(), selection: None }
    }

    pub fn select(mut self, anchor: usize, head: usize) -> Self {
        self.selection = Some((anchor, head));
        self
    }

    fn select_text(self, reversed: bool) -> Self {
        let len = self.text.chars().count();
        let selection = HelixSelection::spanning(0, len, reversed);
        self.select(selection.anchor, selection.head)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HelixRangeMode {
    Selection,
    Before,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelixWordMotion {
    NextWordStart,
    NextWordEnd,
    PrevWordStart,
    NextLongWordStart,
    NextLongWordEnd,
    PrevLongWordStart,
}

impl HelixWordMotion {
    fn is_prev(self) -> bool {
        matches!(self, Self::PrevWordStart | Self::PrevLongWordStart)
    }

    fn reached(self, prev: char, next: char) -> bool {
        match self {
            Self::NextWordStart => is_word_boundary(prev, next) && (next == '\n' || !next.is_whitespace()),
            Self::NextWordEnd | Self::PrevWordStart => is_word_boundary(prev, next) && (!prev.is_whitespace() || next == '\n'),
            Self::NextLongWordStart => is_long_word_boundary(prev, next) && (next == '\n' || !next.is_whitespace()),
            Self::NextLongWordEnd | Self::PrevLongWordStart => is_long_word_boundary(prev, next) && (!prev.is_whitespace() || next == '\n'),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharCategory {
    Eol,
    Whitespace,
    Word,
    Punctuation,
}

fn categorize(character: char) -> CharCategory {
    if character == '\n' {
        CharCategory::Eol
    } else if character.is_whitespace() {
        CharCategory::Whitespace
    } else if character.is_alphanumeric() || character == '_' {
        CharCategory::Word
    } else {
        CharCategory::Punctuation
    }
}

fn is_word_boundary(a: char, b: char) -> bool {
    categorize(a) != categorize(b)
}

fn is_long_word_boundary(a: char, b: char) -> bool {
    match (categorize(a), categorize(b)) {
        (CharCategory::Word, CharCategory::Punctuation) | (CharCategory::Punctuation, CharCategory::Word) => false,
        (a, b) => a != b,
    }
}

fn to_gap(selection: HelixSelection, len: usize) -> (usize, usize) {
    if selection.is_reversed() {
        ((selection.anchor + 1).min(len), selection.head.min(len))
    } else {
        (selection.anchor.min(len), (selection.head + 1).min(len))
    }
}

fn from_gap((anchor, head): (usize, usize), len: usize) -> HelixSelection {
    match anchor.cmp(&head) {
        Ordering::Less => HelixSelection { anchor, head: head - 1 },
        Ordering::Greater => HelixSelection { anchor: anchor - 1, head },
        Ordering::Equal => HelixSelection::caret(head.min(len)),
    }
}

fn word_move(chars: &[char], (anchor, head): (usize, usize), count: usize, motion: HelixWordMotion) -> (usize, usize) {
    let len = chars.len();
    let is_prev = motion.is_prev();
    if (is_prev && head == 0) || (!is_prev && head == len) {
        return (anchor, head);
    }
    let mut range = match (is_prev, anchor < head) {
        (true, true) => (head, head - 1),
        (true, false) => ((head + 1).min(len), head),
        (false, true) => (head - 1, head),
        (false, false) => (head, (head + 1).min(len)),
    };
    for _ in 0..count.max(1) {
        let next = range_to_target(chars, motion, range);
        if next == range {
            break;
        }
        range = next;
    }
    range
}

fn range_to_target(chars: &[char], motion: HelixWordMotion, (mut anchor, mut head): (usize, usize)) -> (usize, usize) {
    let is_prev = motion.is_prev();
    let len = chars.len();
    let mut position = head;
    let next = |position: &mut usize| -> Option<char> {
        if is_prev {
            (*position > 0).then(|| {
                *position -= 1;
                chars[*position]
            })
        } else {
            (*position < len).then(|| {
                *position += 1;
                chars[*position - 1]
            })
        }
    };
    let step_back = |position: &mut usize| {
        if is_prev {
            *position = (*position + 1).min(len);
        } else {
            *position = position.saturating_sub(1);
        }
    };
    let advance = |head: &mut usize| {
        if is_prev {
            *head = head.saturating_sub(1);
        } else {
            *head += 1;
        }
    };
    let mut prev_char = if is_prev { chars.get(position).copied() } else { position.checked_sub(1).map(|index| chars[index]) };
    while let Some(character) = next(&mut position) {
        if character == '\n' {
            prev_char = Some(character);
            advance(&mut head);
        } else {
            step_back(&mut position);
            break;
        }
    }
    if prev_char == Some('\n') {
        anchor = head;
    }
    let head_start = head;
    while let Some(next_char) = next(&mut position) {
        if prev_char.is_none_or(|prev_char| motion.reached(prev_char, next_char)) {
            if head == head_start {
                anchor = head;
            } else {
                break;
            }
        }
        prev_char = Some(next_char);
        advance(&mut head);
    }
    (anchor, head)
}

fn text_len_in(starts: &[usize]) -> usize {
    starts.last().copied().unwrap_or(1).saturating_sub(1)
}

fn offset_in(starts: &[usize], pos: Position) -> usize {
    if starts.len() < 2 {
        return 0;
    }
    let row = pos.row.min(starts.len() - 2);
    let line_len = starts[row + 1] - starts[row] - 1;
    starts[row] + pos.col.min(line_len)
}

fn position_in(starts: &[usize], offset: usize) -> Position {
    if starts.len() < 2 {
        return Position::new(0, 0);
    }
    let row = starts.partition_point(|start| *start <= offset).saturating_sub(1).min(starts.len() - 2);
    let line_len = starts[row + 1] - starts[row] - 1;
    Position::new(row, offset.saturating_sub(starts[row]).min(line_len))
}

impl Editor {
    pub fn helix_active(&self) -> bool {
        self.helix_selections.is_some()
    }

    pub fn helix_begin(&mut self) {
        let offset = self.helix_offset(self.cursor.pos());
        self.cursor.cancel_selection();
        self.helix_set_selections(vec![HelixSelection::caret(offset)], 0);
    }

    pub fn helix_end(&mut self) {
        self.helix_commit_transaction();
        self.helix_selections = None;
        self.helix_render_ranges.clear();
        self.helix_render_heads.clear();
    }

    pub fn helix_selection_set(&self) -> Option<&HelixSelectionSet> {
        self.helix_selections.as_ref()
    }

    pub fn helix_text_len(&self) -> usize {
        text_len_in(&self.buffer.line_starts())
    }

    pub fn helix_offset(&self, pos: Position) -> usize {
        offset_in(&self.buffer.line_starts(), pos)
    }

    pub fn helix_position(&self, offset: usize) -> Position {
        position_in(&self.buffer.line_starts(), offset)
    }

    pub fn helix_line_start(&self, row: usize) -> usize {
        self.helix_offset(Position::new(row, 0))
    }

    pub fn helix_line_end(&self, row: usize) -> usize {
        self.helix_offset(Position::new(row, usize::MAX))
    }

    pub fn helix_chars(&self) -> Vec<char> {
        let mut chars = Vec::with_capacity(self.helix_text_len());
        for (row, line) in self.buffer.iter_lines().enumerate() {
            if row > 0 {
                chars.push('\n');
            }
            chars.extend(line.chars());
        }
        chars
    }

    pub fn helix_slice(&self, start: usize, end: usize) -> String {
        if start >= end {
            return String::new();
        }
        let starts = self.buffer.line_starts();
        let start = position_in(&starts, start);
        let end = position_in(&starts, end);
        self.buffer.get_text_range(start.row, start.col, end.row, end.col)
    }

    pub fn helix_set_selections(&mut self, mut selections: Vec<HelixSelection>, primary: usize) {
        let starts = self.buffer.line_starts();
        let len = text_len_in(&starts);
        if selections.is_empty() {
            selections.push(HelixSelection::caret(offset_in(&starts, self.cursor.pos())));
        }
        for selection in &mut selections {
            selection.anchor = selection.anchor.min(len);
            selection.head = selection.head.min(len);
        }
        let requested_primary = primary.min(selections.len() - 1);
        let mut indexed: Vec<_> = selections.into_iter().enumerate().collect();
        indexed.sort_by_key(|(_, selection)| selection.range(len));
        let mut merged: Vec<HelixSelection> = Vec::with_capacity(indexed.len());
        let mut new_primary = 0;
        for (index, selection) in indexed {
            let is_primary = index == requested_primary;
            if let Some(last) = merged.last_mut() {
                let (last_start, last_end) = last.range(len);
                let (start, end) = selection.range(len);
                if start < last_end || (start == last_start && end == last_end) {
                    let reversed = if is_primary { selection.is_reversed() } else { last.is_reversed() };
                    *last = HelixSelection::spanning(last_start, last_end.max(end), reversed);
                    if is_primary {
                        new_primary = merged.len() - 1;
                    }
                    continue;
                }
            }
            if is_primary {
                new_primary = merged.len();
            }
            merged.push(selection);
        }
        self.helix_render_ranges = merged
            .iter()
            .map(|selection| {
                let (start, end) = selection.range(len);
                (position_in(&starts, start), position_in(&starts, end))
            })
            .collect();
        self.helix_render_heads = merged.iter().enumerate().filter(|(index, _)| *index != new_primary).map(|(_, selection)| position_in(&starts, selection.head)).collect();
        self.helix_render_heads.sort_unstable();
        let head = merged[new_primary].head;
        self.helix_selections = Some(HelixSelectionSet { selections: merged, primary: new_primary });
        let pos = position_in(&starts, head);
        self.preferred_visual_x = None;
        self.set_cursor(pos.row, pos.col);
    }

    pub fn helix_primary(&self) -> Option<HelixSelection> {
        let set = self.helix_selections.as_ref()?;
        set.selections.get(set.primary).copied()
    }

    pub fn helix_selected_texts(&self) -> Vec<String> {
        let Some(set) = self.helix_selections.as_ref() else { return Vec::new() };
        let len = self.helix_text_len();
        set.selections
            .iter()
            .map(|selection| {
                let (start, end) = selection.range(len);
                self.helix_slice(start, end)
            })
            .collect()
    }

    pub fn helix_move(&mut self, movement: CursorMove, extend: bool) {
        let Some(set) = self.helix_selections.clone() else { return };
        let vertical = matches!(movement, CursorMove::Up | CursorMove::Down);
        let sticky = (self.cursor.preferred_col, self.preferred_visual_x);
        let scroll = self.scroll_offset;
        let mut primary_state = (scroll, sticky);
        let mut moved = Vec::with_capacity(set.selections.len());
        for (index, selection) in set.selections.iter().enumerate() {
            let is_primary = index == set.primary;
            let pos = self.helix_position(selection.head);
            self.scroll_offset = scroll;
            self.cursor.set_pos(pos, false);
            if is_primary && vertical {
                (self.cursor.preferred_col, self.preferred_visual_x) = sticky;
            } else {
                self.cursor.preferred_col = Some(pos.col);
                self.preferred_visual_x = None;
            }
            self.apply_cursor_move(movement);
            self.reveal_row(self.cursor.pos().row);
            if is_primary {
                primary_state = (self.scroll_offset, (self.cursor.preferred_col, self.preferred_visual_x));
            }
            let next = self.helix_offset(self.cursor.pos());
            moved.push(if extend { HelixSelection { head: next, ..*selection } } else { HelixSelection::caret(next) });
        }
        self.scroll_offset = primary_state.0;
        self.helix_set_selections(moved, set.primary);
        if vertical {
            (self.cursor.preferred_col, self.preferred_visual_x) = primary_state.1;
        }
    }

    pub fn helix_word_move(&mut self, motion: HelixWordMotion, count: usize, extend: bool) {
        let Some(set) = self.helix_selections.clone() else { return };
        let chars = self.helix_chars();
        let len = chars.len();
        let selections = set
            .selections
            .iter()
            .map(|selection| {
                let moved = from_gap(word_move(&chars, to_gap(*selection, len), count, motion), len);
                if extend {
                    HelixSelection { anchor: selection.anchor, head: moved.head }
                } else {
                    moved
                }
            })
            .collect();
        self.helix_set_selections(selections, set.primary);
    }

    pub fn helix_begin_transaction(&mut self) {
        if self.helix_transaction.is_none() {
            self.helix_transaction = Some((Vec::new(), self.cursor.pos(), self.helix_selections.clone()));
        }
    }

    pub fn helix_commit_transaction(&mut self) {
        if let Some((operations, before_cursor, before_selections)) = self.helix_transaction.take() {
            self.history.record_helix_group(operations, before_cursor, self.cursor.pos(), before_selections, self.helix_selections.clone());
        }
    }

    /// Replace each selected range in one atomic, undoable command.
    pub fn helix_replace(&mut self, replacements: &[String], range_mode: HelixRangeMode) -> bool {
        let Some(set) = self.helix_selections.clone() else { return false };
        let len = self.helix_text_len();
        let changes = set
            .selections
            .iter()
            .enumerate()
            .map(|(index, selection)| {
                let (start, end) = selection.range(len);
                let text = replacements.get(index).or_else(|| replacements.last()).cloned().unwrap_or_default();
                match range_mode {
                    HelixRangeMode::Selection => HelixChange::new(start, end, text).select_text(selection.is_reversed()),
                    HelixRangeMode::Before | HelixRangeMode::After => {
                        let at = if matches!(range_mode, HelixRangeMode::Before) { start } else { end };
                        let caret = text.chars().count();
                        HelixChange::new(at, at, text).select(caret, caret)
                    }
                }
            })
            .collect();
        self.helix_edit(changes, set.primary)
    }

    pub fn helix_edit(&mut self, changes: Vec<HelixChange>, primary: usize) -> bool {
        let Some(before_set) = self.helix_selections.clone() else { return false };
        let before_cursor = self.cursor.pos();
        let starts = self.buffer.line_starts();
        let len = text_len_in(&starts);
        let mut indexed: Vec<(usize, HelixChange)> = changes.into_iter().enumerate().collect();
        indexed.sort_by_key(|(_, change)| change.start);
        let mut previous_end = 0;
        for (_, change) in &mut indexed {
            change.start = change.start.min(len).max(previous_end);
            change.end = change.end.min(len).max(change.start);
            previous_end = change.end;
        }
        let mut shifts = Vec::with_capacity(indexed.len() + 1);
        shifts.push(0isize);
        for (_, change) in &indexed {
            let delta = change.text.chars().count() as isize - (change.end - change.start) as isize;
            shifts.push(shifts[shifts.len() - 1] + delta);
        }
        let new_start = |index: usize| (indexed[index].1.start as isize + shifts[index]) as usize;
        let (selections, new_primary) = if indexed.iter().any(|(_, change)| change.selection.is_some()) {
            let mut selections = Vec::new();
            let mut new_primary = 0;
            for (index, (original, change)) in indexed.iter().enumerate() {
                if let Some((anchor, head)) = change.selection {
                    if *original == primary {
                        new_primary = selections.len();
                    }
                    let start = new_start(index);
                    selections.push(HelixSelection { anchor: start + anchor, head: start + head });
                }
            }
            (selections, new_primary)
        } else {
            let map = |offset: usize| {
                let index = indexed.partition_point(|(_, change)| change.end <= offset);
                if let Some((_, change)) = indexed.get(index).filter(|(_, change)| change.start <= offset) {
                    return new_start(index) + (offset - change.start).min(change.text.chars().count().saturating_sub(1));
                }
                (offset as isize + shifts[index]).max(0) as usize
            };
            (before_set.selections.iter().map(|selection| HelixSelection { anchor: map(selection.anchor), head: map(selection.head) }).collect(), before_set.primary)
        };
        let mut operations = Vec::new();
        for (_, change) in indexed.iter().rev() {
            let start_pos = position_in(&starts, change.start);
            if change.start < change.end {
                let end_pos = position_in(&starts, change.end);
                let deleted = self.buffer.delete_text_range(start_pos.row, start_pos.col, end_pos.row, end_pos.col);
                let removed_rows = end_pos.row - start_pos.row;
                if removed_rows > 0 {
                    self.remap_folds_for_deleted_rows(start_pos.row + 1, removed_rows);
                } else {
                    self.reconcile_fold_anchors();
                }
                self.wrap_cache.invalidate_from(start_pos.row);
                operations.push(EditOperation::Delete { start: start_pos, end: end_pos, deleted_text: deleted });
            }
            if !change.text.is_empty() {
                self.apply_insert(start_pos, &change.text);
                operations.push(EditOperation::Insert { pos: start_pos, text: change.text.clone() });
            }
        }
        if operations.is_empty() {
            return false;
        }
        self.highlight_index.clear();
        self.row_style_cache.borrow_mut().invalidate_all();
        self.recalc_code_blocks_from(0);
        self.update_line_number_width();
        self.helix_set_selections(selections, new_primary);
        if let Some((pending, _, _)) = self.helix_transaction.as_mut() {
            pending.extend(operations);
        } else {
            self.history.record_helix_group(operations, before_cursor, self.cursor.pos(), Some(before_set), self.helix_selections.clone());
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selections(editor: &Editor) -> Vec<HelixSelection> {
        editor.helix_selection_set().unwrap().selections.clone()
    }

    #[test]
    fn multiple_replacements_are_one_undo_step() {
        let mut editor = Editor::from_text("one two\nthree");
        editor.helix_begin();
        editor.helix_set_selections(vec![HelixSelection::caret(0), HelixSelection::caret(4)], 1);
        assert!(editor.helix_replace(&["X".into(), "Y".into()], HelixRangeMode::Selection));
        assert_eq!(editor.text(), "Xne Ywo\nthree");
        assert!(editor.undo());
        assert_eq!(editor.text(), "one two\nthree");
        assert_eq!(editor.helix_selection_set().unwrap().selections.len(), 2);
        assert!(editor.redo());
        assert_eq!(editor.text(), "Xne Ywo\nthree");
    }

    #[test]
    fn insert_transaction_groups_characters() {
        let mut editor = Editor::from_text("ab");
        editor.helix_begin();
        editor.helix_begin_transaction();
        editor.helix_replace(&["x".into()], HelixRangeMode::Before);
        editor.helix_replace(&["y".into()], HelixRangeMode::Before);
        editor.helix_commit_transaction();
        assert_eq!(editor.text(), "xyab");
        assert!(editor.undo());
        assert_eq!(editor.text(), "ab");
        assert!(!editor.undo());
    }

    #[test]
    fn unicode_offsets_and_overlaps_remain_valid() {
        let mut editor = Editor::from_text("é🙂\n猫");
        editor.helix_begin();
        editor.helix_set_selections(vec![HelixSelection { anchor: 1, head: 3 }, HelixSelection::caret(2)], 1);
        assert_eq!(editor.helix_selection_set().unwrap().selections.len(), 1);
        assert_eq!(editor.helix_selected_texts(), vec!["🙂\n猫"]);
        editor.helix_replace(&["ok".into()], HelixRangeMode::Selection);
        assert_eq!(editor.text(), "éok");
        editor.undo();
        assert_eq!(editor.text(), "é🙂\n猫");
    }

    #[test]
    fn overlapping_reversed_selection_keeps_its_full_range() {
        let mut editor = Editor::from_text("abcdef");
        editor.helix_begin();
        editor.helix_set_selections(vec![HelixSelection { anchor: 4, head: 1 }, HelixSelection { anchor: 2, head: 3 }], 0);
        assert_eq!(editor.helix_selected_texts(), vec!["bcde"]);
        assert_eq!(editor.helix_primary().unwrap().head, 1);
        editor.helix_replace(&["X".into()], HelixRangeMode::Selection);
        assert_eq!(editor.text(), "aXf");
    }

    #[test]
    fn duplicate_carets_at_end_of_text_stay_carets() {
        let mut editor = Editor::from_text("ab");
        editor.helix_begin();
        editor.helix_set_selections(vec![HelixSelection::caret(2), HelixSelection::caret(2)], 1);
        assert_eq!(selections(&editor), vec![HelixSelection::caret(2)]);
    }

    #[test]
    fn replacing_a_selection_selects_the_new_text() {
        let mut editor = Editor::from_text("hello world");
        editor.helix_begin();
        editor.helix_set_selections(vec![HelixSelection { anchor: 4, head: 0 }], 0);
        editor.helix_replace(&["HEY".into()], HelixRangeMode::Selection);
        assert_eq!(editor.text(), "HEY world");
        assert_eq!(selections(&editor), vec![HelixSelection { anchor: 2, head: 0 }]);
        editor.helix_replace(&[String::new()], HelixRangeMode::Selection);
        assert_eq!(selections(&editor), vec![HelixSelection::caret(0)]);
    }

    #[test]
    fn mapped_edits_keep_selections_on_their_text() {
        let mut editor = Editor::from_text("ab\ncd");
        editor.helix_begin();
        editor.helix_set_selections(vec![HelixSelection::caret(1), HelixSelection::caret(4)], 1);
        assert!(editor.helix_edit(vec![HelixChange::new(0, 0, "\t"), HelixChange::new(3, 3, "\t")], 0));
        assert_eq!(editor.text(), "\tab\n\tcd");
        assert_eq!(selections(&editor), vec![HelixSelection::caret(2), HelixSelection::caret(6)]);
        assert_eq!(editor.helix_selection_set().unwrap().primary, 1);
    }

    #[test]
    fn overlapping_deletions_are_unioned() {
        let mut editor = Editor::from_text("abcdef");
        editor.helix_begin();
        assert!(editor.helix_edit(vec![HelixChange::new(0, 3, "").select(0, 0), HelixChange::new(0, 5, "").select(0, 0)], 0));
        assert_eq!(editor.text(), "f");
        assert_eq!(selections(&editor), vec![HelixSelection::caret(0)]);
    }

    #[test]
    fn word_motions_select_like_helix() {
        let mut editor = Editor::from_text("one two three");
        editor.helix_begin();
        editor.helix_word_move(HelixWordMotion::NextWordStart, 1, false);
        assert_eq!(editor.helix_selected_texts(), vec!["one "]);
        editor.helix_word_move(HelixWordMotion::NextWordStart, 1, false);
        assert_eq!(editor.helix_selected_texts(), vec!["two "]);
        editor.helix_word_move(HelixWordMotion::NextWordEnd, 1, false);
        assert_eq!(editor.helix_selected_texts(), vec![" three"]);
        editor.helix_word_move(HelixWordMotion::PrevWordStart, 1, false);
        assert_eq!(editor.helix_selected_texts(), vec!["three"]);
        assert!(editor.helix_primary().unwrap().is_reversed());
        editor.helix_word_move(HelixWordMotion::PrevWordStart, 2, false);
        assert_eq!(editor.helix_selected_texts(), vec!["one "]);
    }

    #[test]
    fn word_motions_extend_and_cross_lines() {
        let mut editor = Editor::from_text("foo.bar baz\nqux");
        editor.helix_begin();
        editor.helix_word_move(HelixWordMotion::NextLongWordStart, 1, false);
        assert_eq!(editor.helix_selected_texts(), vec!["foo.bar "]);
        editor.helix_set_selections(vec![HelixSelection::caret(0)], 0);
        editor.helix_word_move(HelixWordMotion::NextWordEnd, 2, true);
        assert_eq!(editor.helix_selected_texts(), vec!["foo."]);
        editor.helix_set_selections(vec![HelixSelection::caret(8)], 0);
        editor.helix_word_move(HelixWordMotion::NextWordStart, 1, false);
        assert_eq!(editor.helix_selected_texts(), vec!["baz"]);
        editor.helix_word_move(HelixWordMotion::NextWordStart, 1, false);
        assert_eq!(editor.helix_selected_texts(), vec!["qux"]);
    }

    #[test]
    fn vertical_moves_keep_each_cursor_column() {
        let mut editor = Editor::from_text("abcdef\nab\nabcdef\nabcdef");
        editor.helix_begin();
        editor.helix_set_selections(vec![HelixSelection::caret(4), HelixSelection::caret(12)], 0);
        editor.helix_move(CursorMove::Down, false);
        let positions: Vec<_> = selections(&editor).iter().map(|selection| editor.helix_position(selection.head)).collect();
        assert_eq!(positions, vec![Position::new(1, 2), Position::new(3, 2)]);
        editor.helix_move(CursorMove::Down, false);
        assert_eq!(editor.helix_position(editor.helix_primary().unwrap().head), Position::new(2, 4));
    }

    #[test]
    fn offsets_round_trip_through_positions() {
        let editor = Editor::from_text("ab\n\ncé");
        assert_eq!(editor.helix_text_len(), 6);
        for offset in 0..=6 {
            assert_eq!(editor.helix_offset(editor.helix_position(offset)), offset);
        }
        assert_eq!(editor.helix_position(3), Position::new(1, 0));
        assert_eq!(editor.helix_position(99), Position::new(2, 2));
        assert_eq!(editor.helix_line_end(0), 2);
    }
}
