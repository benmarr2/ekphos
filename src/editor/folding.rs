use super::*;

impl Editor {
    /// Return the ATX heading level for an editor-foldable source row.
    /// Ekphos intentionally matches preview mode by folding H1-H3 only.
    pub fn foldable_heading_level(&self, row: usize) -> Option<usize> {
        self.ensure_fold_projection();
        self.fold_projection.borrow().as_ref().and_then(|projection| projection.heading_levels.get(row).copied().flatten())
    }

    pub fn is_heading_folded(&self, row: usize) -> bool {
        self.folded_headings.contains(&row) && self.foldable_heading_level(row).is_some()
    }

    pub fn toggle_heading_fold(&mut self, row: usize) -> bool {
        self.reconcile_fold_anchors();
        if self.foldable_heading_level(row).is_none() || self.heading_section_end(row) == row + 1 {
            return false;
        }
        if !self.folded_headings.remove(&row) {
            self.folded_headings.insert(row);
        }
        self.invalidate_fold_projection();
        self.scroll_offset = self.normalize_scroll_row(self.scroll_offset);
        self.ensure_cursor_visible();
        true
    }

    pub fn toggle_current_heading_fold(&mut self) -> bool {
        self.toggle_heading_fold(self.cursor.pos().row)
    }

    pub(super) fn unfold_heading(&mut self, row: usize) -> bool {
        if !self.folded_headings.remove(&row) {
            return false;
        }
        self.invalidate_fold_projection();
        self.scroll_offset = self.normalize_scroll_row(self.scroll_offset);
        self.ensure_cursor_visible();
        true
    }

    pub fn fold_all_headings(&mut self) -> usize {
        self.reconcile_fold_anchors();
        let rows: Vec<_> = (0..self.buffer.line_count()).filter(|&row| self.foldable_heading_level(row).is_some() && self.heading_section_end(row) > row + 1).collect();
        let count = rows.len();
        self.folded_headings.extend(rows);
        self.invalidate_fold_projection();
        let cursor_row = self.cursor.pos().row;
        if let Some(parent) = self.enclosing_folded_heading(cursor_row) {
            self.cursor.move_to(parent, self.buffer.line_len(parent));
        }
        self.scroll_offset = self.normalize_scroll_row(self.scroll_offset);
        self.ensure_cursor_visible();
        count
    }

    pub fn unfold_all_headings(&mut self) -> usize {
        let count = self.folded_headings.len();
        self.folded_headings.clear();
        self.invalidate_fold_projection();
        self.ensure_cursor_visible();
        count
    }

    pub fn is_row_hidden(&self, row: usize) -> bool {
        self.hidden_range_containing(row).is_some()
    }

    pub fn reveal_row(&mut self, row: usize) -> bool {
        let hidden_by: Vec<_> = self.folded_headings.iter().copied().filter(|&heading| heading < row && row < self.heading_section_end(heading)).collect();
        let changed = !hidden_by.is_empty();
        for heading in hidden_by {
            self.folded_headings.remove(&heading);
        }
        if changed {
            self.invalidate_fold_projection();
        }
        changed
    }

    pub fn visible_row_distance(&self, start: usize, end: usize) -> usize {
        let end = end.min(self.buffer.line_count());
        if start >= end {
            return 0;
        }
        if self.folded_headings.is_empty() {
            return end - start;
        }
        self.ensure_fold_projection();
        let projection = self.fold_projection.borrow();
        let ranges = &projection.as_ref().expect("fold projection initialized").hidden_ranges;
        let hidden_rows = ranges.iter().map(|&(hidden_start, hidden_end)| hidden_end.min(end).saturating_sub(hidden_start.max(start))).sum::<usize>();
        end - start - hidden_rows
    }

    pub fn visible_row_at_offset(&self, start: usize, offset: isize) -> usize {
        let start = self.normalize_scroll_row(start.min(self.buffer.line_count().saturating_sub(1)));
        if offset >= 0 {
            let mut row = start;
            for _ in 0..offset as usize {
                let Some(next) = self.next_visible_row(row) else { break };
                row = next;
            }
            row
        } else {
            let mut row = start;
            for _ in 0..offset.unsigned_abs() {
                let Some(previous) = self.previous_visible_row(row) else { break };
                row = previous;
            }
            row
        }
    }

    pub fn visible_source_window(&self, start: usize, rows_before: usize, rows_after: usize) -> std::ops::Range<usize> {
        let first = self.visible_row_at_offset(start, -(rows_before as isize));
        let last = self.visible_row_at_offset(start, rows_after.saturating_sub(1) as isize);
        first..last.saturating_add(1).min(self.buffer.line_count())
    }

    pub(super) fn next_visible_row(&self, row: usize) -> Option<usize> {
        let candidate = row.saturating_add(1);
        if candidate >= self.buffer.line_count() {
            return None;
        }
        if let Some((_, hidden_end)) = self.hidden_range_containing(candidate) {
            (hidden_end < self.buffer.line_count()).then_some(hidden_end)
        } else {
            Some(candidate)
        }
    }

    pub(super) fn previous_visible_row(&self, row: usize) -> Option<usize> {
        let candidate = row.checked_sub(1)?;
        if let Some((hidden_start, _)) = self.hidden_range_containing(candidate) {
            hidden_start.checked_sub(1)
        } else {
            Some(candidate)
        }
    }

    pub(super) fn last_visible_row(&self) -> usize {
        let last = self.buffer.line_count().saturating_sub(1);
        self.hidden_range_containing(last).map_or(last, |(hidden_start, _)| hidden_start.saturating_sub(1))
    }

    pub(super) fn normalize_scroll_row(&self, row: usize) -> usize {
        let last = self.buffer.line_count().saturating_sub(1);
        let row = row.min(last);
        if !self.is_row_hidden(row) {
            return row;
        }
        let (hidden_start, hidden_end) = self.hidden_range_containing(row).expect("hidden row has a range");
        if hidden_end <= last {
            hidden_end
        } else {
            hidden_start.saturating_sub(1)
        }
    }

    pub(super) fn remap_folds_for_inserted_rows(&mut self, row: usize, count: usize) {
        if count == 0 {
            return;
        }
        self.folded_headings = self.folded_headings.iter().map(|&heading| if heading >= row { heading + count } else { heading }).collect();
        self.reconcile_fold_anchors();
    }

    pub(super) fn remap_folds_for_deleted_rows(&mut self, row: usize, count: usize) {
        if count == 0 {
            return;
        }
        let end = row.saturating_add(count);
        self.folded_headings = self
            .folded_headings
            .iter()
            .filter_map(|&heading| {
                if heading < row {
                    Some(heading)
                } else if heading >= end {
                    Some(heading - count)
                } else {
                    None
                }
            })
            .collect();
        self.reconcile_fold_anchors();
    }

    pub(super) fn reconcile_fold_anchors(&mut self) {
        self.invalidate_fold_projection();
        if self.folded_headings.is_empty() {
            self.scroll_offset = self.scroll_offset.min(self.buffer.line_count().saturating_sub(1));
            return;
        }
        self.ensure_fold_projection();
        let valid: BTreeSet<_> = {
            let projection = self.fold_projection.borrow();
            let levels = &projection.as_ref().expect("fold projection initialized").heading_levels;
            self.folded_headings
                .iter()
                .copied()
                .filter(|&row| {
                    let Some(Some(level)) = levels.get(row) else {
                        return false;
                    };
                    let section_end = ((row + 1)..levels.len()).find(|&candidate| levels[candidate].is_some_and(|candidate_level| candidate_level <= *level)).unwrap_or(levels.len());
                    section_end > row + 1
                })
                .collect()
        };
        self.folded_headings = valid;
        self.invalidate_fold_projection();
        self.scroll_offset = self.normalize_scroll_row(self.scroll_offset);
    }

    fn enclosing_folded_heading(&self, row: usize) -> Option<usize> {
        self.folded_headings.range(..row).rev().copied().find(|&heading| self.foldable_heading_level(heading).is_some() && row < self.heading_section_end(heading))
    }

    pub(super) fn heading_section_end(&self, row: usize) -> usize {
        let Some(level) = self.foldable_heading_level(row) else {
            return row.saturating_add(1).min(self.buffer.line_count());
        };
        let projection = self.fold_projection.borrow();
        let levels = &projection.as_ref().expect("fold projection initialized").heading_levels;
        ((row + 1)..levels.len()).find(|&candidate| levels[candidate].is_some_and(|candidate_level| candidate_level <= level)).unwrap_or(levels.len())
    }

    fn ensure_fold_projection(&self) {
        if self.fold_projection.borrow().is_some() {
            return;
        }
        let line_count = self.buffer.line_count();
        let frontmatter_end = crate::core::markdown::frontmatter_end_in_lines(self.buffer.iter_lines());
        let mut heading_levels = vec![None; line_count];
        let mut fence: Option<&'static str> = None;
        for (row, line) in self.buffer.iter_lines().enumerate() {
            if frontmatter_end.is_some_and(|end| row <= end) {
                continue;
            }
            let trimmed = line.trim_start();
            let marker = if trimmed.starts_with("```") {
                Some("```")
            } else if trimmed.starts_with("~~~") {
                Some("~~~")
            } else {
                None
            };
            if fence.is_none() && marker.is_none() {
                if let Some(heading) = crate::core::markdown::heading(line) {
                    if heading.level <= 3 && line[heading.level..].starts_with(' ') {
                        heading_levels[row] = Some(heading.level);
                    }
                }
            }
            match (fence, marker) {
                (None, Some(opened)) => fence = Some(opened),
                (Some(opened), Some(current)) if opened == current => fence = None,
                _ => {}
            }
        }

        let mut open_folds: Vec<(usize, usize)> = Vec::new();
        let mut hidden_ranges = Vec::new();
        for (row, level) in heading_levels.iter().copied().enumerate() {
            let Some(level) = level else { continue };
            while open_folds.last().is_some_and(|&(_, open_level)| open_level >= level) {
                let (heading, _) = open_folds.pop().expect("checked above");
                if heading + 1 < row {
                    hidden_ranges.push((heading + 1, row));
                }
            }
            if self.folded_headings.contains(&row) {
                open_folds.push((row, level));
            }
        }
        for (heading, _) in open_folds {
            if heading + 1 < line_count {
                hidden_ranges.push((heading + 1, line_count));
            }
        }
        hidden_ranges.sort_unstable();
        let mut merged_ranges: Vec<(usize, usize)> = Vec::with_capacity(hidden_ranges.len());
        for (start, end) in hidden_ranges {
            if let Some((_, previous_end)) = merged_ranges.last_mut() {
                if start <= *previous_end {
                    *previous_end = (*previous_end).max(end);
                    continue;
                }
            }
            merged_ranges.push((start, end));
        }
        *self.fold_projection.borrow_mut() = Some(FoldProjection { heading_levels, hidden_ranges: merged_ranges });
    }

    fn invalidate_fold_projection(&mut self) {
        self.fold_projection.get_mut().take();
    }

    fn hidden_range_containing(&self, row: usize) -> Option<(usize, usize)> {
        if self.folded_headings.is_empty() {
            return None;
        }
        self.ensure_fold_projection();
        let projection = self.fold_projection.borrow();
        let ranges = &projection.as_ref().expect("fold projection initialized").hidden_ranges;
        let index = ranges.partition_point(|&(_, end)| end <= row);
        ranges.get(index).copied().filter(|&(start, end)| start <= row && row < end)
    }

    pub(super) fn is_frontmatter_row(&self, row: usize) -> bool {
        crate::core::markdown::frontmatter_end_in_lines(self.buffer.iter_lines()).is_some_and(|end| row <= end)
    }

    pub(super) fn is_fenced_code_row(&self, row: usize) -> bool {
        let mut fence: Option<&str> = None;
        for (index, line) in self.buffer.iter_lines().enumerate().take(row.saturating_add(1)) {
            let trimmed = line.trim_start();
            let marker = if trimmed.starts_with("```") {
                Some("```")
            } else if trimmed.starts_with("~~~") {
                Some("~~~")
            } else {
                None
            };
            if index == row {
                return fence.is_some() || marker.is_some();
            }
            match (fence, marker) {
                (None, Some(opened)) => fence = Some(opened),
                (Some(opened), Some(current)) if opened == current => fence = None,
                _ => {}
            }
        }
        false
    }
}
