use super::*;
use crate::editor::{HelixChange, HelixRangeMode, HelixSelection, HelixSelectionSet, HelixWordMotion};
use crate::helix::HelixMode;
use regex::{Regex, RegexBuilder};
use std::sync::LazyLock;

static MARKDOWN_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(!?)\[[^\]]*\]\(\s*<?([^)\s>]+)>?[^)]*\)").expect("valid markdown link pattern"));
static BARE_URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"https?://[^\s<>()\[\]]+").expect("valid url pattern"));

pub(super) fn handle_helix_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    app.editor.helix.status_message = None;
    if let Some(register) = app.editor.helix.recording {
        if !app.editor.helix.playing_macro {
            let keys = app.editor.helix.macros.entry(register).or_default();
            if keys.len() < 10_000 {
                keys.push(key);
            }
        }
    }
    match app.editor.helix.mode {
        HelixMode::Insert => handle_helix_insert(app, key),
        mode if mode.is_prompt() => handle_helix_prompt(app, key),
        _ => handle_helix_normal(app, key),
    }
    app.request_highlight_update();
    app.update_editor_block();
}

pub(super) fn helix_reset_input(app: &mut App) {
    app.editor.helix_commit_transaction();
    let helix = &mut app.editor.helix;
    helix.mode = HelixMode::Normal;
    helix.pending = None;
    helix.count.clear();
    helix.prompt.clear();
    helix.surround_from = None;
    helix.sticky_view = false;
    helix.restore_cursor = false;
    update_cursor_style(app);
}

pub(super) fn helix_insert_pasted_text(app: &mut App, text: String) {
    if app.editor.helix.mode == HelixMode::Insert {
        app.editor.helix_replace(std::slice::from_ref(&text), HelixRangeMode::Before);
        app.editor.helix.last_insert.push_str(&text);
    } else {
        paste_values(app, &[text], true);
    }
}

pub(super) fn helix_select_all(app: &mut App) {
    let len = app.editor.helix_text_len();
    app.editor.helix_set_selections(vec![HelixSelection::spanning(0, len, false)], 0);
}

fn status(app: &mut App, message: impl Into<String>) {
    app.editor.helix.status_message = Some(message.into());
}

fn selection_set(app: &App) -> Option<HelixSelectionSet> {
    app.editor.helix_selection_set().cloned()
}

fn push_jump(app: &mut App) {
    if let Some(set) = selection_set(app) {
        app.editor.helix.push_jump(set);
    }
}

fn register_values(app: &mut App, register: char) -> Vec<String> {
    if register == '+' || register == '*' {
        match app.clipboard_image_link() {
            Some(Ok(link)) => return vec![link],
            Some(Err(error)) => {
                status(app, error);
                return Vec::new();
            }
            None => {}
        }
        match app.clipboard().get_text() {
            Ok(Some(text)) => vec![text],
            Ok(None) => Vec::new(),
            Err(error) => {
                status(app, format!("Clipboard: {error}"));
                Vec::new()
            }
        }
    } else {
        app.editor.helix.registers.get(&register).cloned().unwrap_or_default()
    }
}

fn line_range(app: &App, selection: HelixSelection, len: usize) -> (usize, usize) {
    let (from, to) = selection.range(len);
    (app.editor.helix_position(from).row, app.editor.helix_position(to.saturating_sub(1).max(from)).row)
}

fn line_bound(app: &App, row: usize) -> usize {
    if row + 1 < app.editor.line_count() {
        app.editor.helix_line_start(row + 1)
    } else {
        app.editor.helix_text_len()
    }
}

fn line_indent(app: &App, row: usize) -> String {
    app.editor.line(row).map(|line| line.chars().take_while(|character| *character == ' ' || *character == '\t').collect()).unwrap_or_default()
}

fn begin_insert(app: &mut App) {
    app.editor.helix_begin_transaction();
    app.editor.helix.mode = HelixMode::Insert;
    app.editor.helix.last_insert.clear();
    app.editor.helix.restore_cursor = false;
    update_cursor_style(app);
}

fn collapse_to(app: &mut App, end: bool) {
    let Some(set) = selection_set(app) else { return };
    let len = app.editor.helix_text_len();
    let carets = set
        .selections
        .iter()
        .map(|selection| {
            let (start, stop) = selection.range(len);
            HelixSelection::caret(if end { stop } else { start })
        })
        .collect();
    app.editor.helix_set_selections(carets, set.primary);
}

fn exit_insert(app: &mut App) {
    if std::mem::take(&mut app.editor.helix.restore_cursor) {
        if let Some(set) = selection_set(app) {
            let carets = set.selections.iter().map(|selection| HelixSelection::caret(selection.head.saturating_sub(1))).collect();
            app.editor.helix_set_selections(carets, set.primary);
        }
    }
    app.editor.helix_commit_transaction();
    app.editor.helix.mode = HelixMode::Normal;
    app.editor.helix.pending = None;
    update_cursor_style(app);
}

fn insert_text(app: &mut App, text: &str) {
    app.editor.helix_replace(&[text.to_string()], HelixRangeMode::Before);
    app.editor.helix.last_insert.push_str(text);
}

fn insert_newline(app: &mut App) {
    let Some(set) = selection_set(app) else { return };
    let len = app.editor.helix_text_len();
    let values: Vec<String> = set
        .selections
        .iter()
        .map(|selection| {
            let pos = app.editor.helix_position(selection.range(len).0);
            let indent: String = line_indent(app, pos.row).chars().take(pos.col).collect();
            format!("\n{indent}")
        })
        .collect();
    app.editor.helix_replace(&values, HelixRangeMode::Before);
    app.editor.helix.last_insert.push('\n');
}

fn handle_helix_insert(app: &mut App, key: crossterm::event::KeyEvent) {
    if app.editor.helix.pending.take() == Some('R') {
        if let KeyCode::Char(register) = key.code {
            let values = register_values(app, register);
            if !values.is_empty() {
                app.editor.helix_replace(&values, HelixRangeMode::Before);
            }
        }
        return;
    }
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match key.code {
        KeyCode::Esc => exit_insert(app),
        KeyCode::Char('s') if control => {
            app.editor.helix_commit_transaction();
            app.editor.helix_begin_transaction();
        }
        KeyCode::Char('r') if control => app.editor.helix.pending = Some('R'),
        KeyCode::Char('v') if control => paste_into_editor(app, None),
        KeyCode::Char('w') if control => delete_backward(app, BackwardDelete::Word),
        KeyCode::Char('u') if control => delete_backward(app, BackwardDelete::Line),
        KeyCode::Char('h') if control => delete_backward(app, BackwardDelete::Char),
        KeyCode::Char('k') if control => delete_to_line_end(app),
        KeyCode::Char('d') if control => delete_forward_char(app),
        KeyCode::Char('j') if control => insert_newline(app),
        KeyCode::Backspace if alt => delete_backward(app, BackwardDelete::Word),
        KeyCode::Char('d') if alt => delete_forward_word(app),
        KeyCode::Backspace => delete_backward(app, BackwardDelete::Char),
        KeyCode::Delete => delete_forward_char(app),
        KeyCode::Enter => insert_newline(app),
        KeyCode::Tab => insert_text(app, "\t"),
        KeyCode::Left => app.editor.helix_move(CursorMove::Back, false),
        KeyCode::Right => app.editor.helix_move(CursorMove::Forward, false),
        KeyCode::Up => app.editor.helix_move(CursorMove::Up, false),
        KeyCode::Down => app.editor.helix_move(CursorMove::Down, false),
        KeyCode::Home => app.editor.helix_move(CursorMove::Head, false),
        KeyCode::End => app.editor.helix_move(CursorMove::End, false),
        KeyCode::Char(character) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) => insert_text(app, &character.to_string()),
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum BackwardDelete {
    Char,
    Word,
    Line,
}

fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn delete_backward(app: &mut App, kind: BackwardDelete) {
    let Some(set) = selection_set(app) else { return };
    let chars = app.editor.helix_chars();
    let changes = set
        .selections
        .iter()
        .map(|selection| {
            let cursor = selection.range(chars.len()).0;
            let start = match kind {
                BackwardDelete::Char => cursor.saturating_sub(1),
                BackwardDelete::Line => {
                    let row = app.editor.helix_position(cursor).row;
                    let line_start = app.editor.helix_line_start(row);
                    let first_non_blank = line_start + line_indent(app, row).chars().count();
                    if cursor == line_start {
                        cursor.saturating_sub(1)
                    } else if first_non_blank < cursor && first_non_blank < app.editor.helix_line_end(row) {
                        first_non_blank
                    } else {
                        line_start
                    }
                }
                BackwardDelete::Word => {
                    let mut start = cursor;
                    while start > 0 && chars[start - 1].is_whitespace() {
                        start -= 1;
                    }
                    if start > 0 {
                        let word = is_word_char(chars[start - 1]);
                        while start > 0 && !chars[start - 1].is_whitespace() && is_word_char(chars[start - 1]) == word {
                            start -= 1;
                        }
                    }
                    start
                }
            };
            HelixChange::new(start, cursor, "").select(0, 0)
        })
        .collect();
    app.editor.helix_edit(changes, set.primary);
}

fn delete_to_line_end(app: &mut App) {
    let Some(set) = selection_set(app) else { return };
    let len = app.editor.helix_text_len();
    let changes = set
        .selections
        .iter()
        .map(|selection| {
            let cursor = selection.range(len).0;
            let row = app.editor.helix_position(cursor).row;
            let line_end = app.editor.helix_line_end(row);
            let end = if cursor == line_end { line_bound(app, row) } else { line_end };
            HelixChange::new(cursor, end, "").select(0, 0)
        })
        .collect();
    app.editor.helix_edit(changes, set.primary);
}

fn delete_forward_char(app: &mut App) {
    let Some(set) = selection_set(app) else { return };
    let len = app.editor.helix_text_len();
    let changes = set
        .selections
        .iter()
        .map(|selection| {
            let cursor = selection.range(len).0;
            HelixChange::new(cursor, (cursor + 1).min(len), "").select(0, 0)
        })
        .collect();
    app.editor.helix_edit(changes, set.primary);
}

fn delete_forward_word(app: &mut App) {
    let Some(set) = selection_set(app) else { return };
    let chars = app.editor.helix_chars();
    let changes = set
        .selections
        .iter()
        .map(|selection| {
            let start = selection.range(chars.len()).0;
            let mut end = start;
            if end < chars.len() {
                let word = is_word_char(chars[end]);
                while end < chars.len() && !chars[end].is_whitespace() && is_word_char(chars[end]) == word {
                    end += 1;
                }
                while end < chars.len() && chars[end].is_whitespace() && chars[end] != '\n' {
                    end += 1;
                }
            }
            HelixChange::new(start, end, "").select(0, 0)
        })
        .collect();
    app.editor.helix_edit(changes, set.primary);
}

fn word_motion(code: KeyCode) -> Option<HelixWordMotion> {
    match code {
        KeyCode::Char('w') => Some(HelixWordMotion::NextWordStart),
        KeyCode::Char('e') => Some(HelixWordMotion::NextWordEnd),
        KeyCode::Char('b') => Some(HelixWordMotion::PrevWordStart),
        KeyCode::Char('W') => Some(HelixWordMotion::NextLongWordStart),
        KeyCode::Char('E') => Some(HelixWordMotion::NextLongWordEnd),
        KeyCode::Char('B') => Some(HelixWordMotion::PrevLongWordStart),
        _ => None,
    }
}

fn handle_helix_normal(app: &mut App, key: crossterm::event::KeyEvent) {
    let extend = app.editor.helix.mode == HelixMode::Select;
    if app.editor.helix.sticky_view {
        match key.code {
            KeyCode::Esc => app.editor.helix.sticky_view = false,
            KeyCode::Char(character) => handle_view_key(app, character),
            _ => {}
        }
        return;
    }
    if let Some(prefix) = app.editor.helix.pending.take() {
        handle_helix_prefix(app, prefix, key);
        return;
    }
    if key.modifiers.contains(KeyModifiers::SUPER) {
        return;
    }
    if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
        if let KeyCode::Char(digit @ '0'..='9') = key.code {
            if digit != '0' || !app.editor.helix.count.is_empty() {
                if app.editor.helix.count.len() < 5 {
                    app.editor.helix.count.push(digit);
                }
                return;
            }
        }
    }
    let had_count = !app.editor.helix.count.is_empty();
    let count = app.editor.helix.take_count();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        handle_helix_control(app, key, count, extend);
        return;
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        handle_helix_alt(app, key, count, extend);
        return;
    }
    if let Some(motion) = word_motion(key.code) {
        app.editor.helix_word_move(motion, count, extend);
        return;
    }
    let movement = match key.code {
        KeyCode::Char('h') | KeyCode::Left => Some(CursorMove::Back),
        KeyCode::Char('j') | KeyCode::Down => Some(CursorMove::Down),
        KeyCode::Char('k') | KeyCode::Up => Some(CursorMove::Up),
        KeyCode::Char('l') | KeyCode::Right => Some(CursorMove::Forward),
        KeyCode::Home => Some(CursorMove::Head),
        KeyCode::End => Some(CursorMove::End),
        KeyCode::PageUp => Some(CursorMove::PageUp),
        KeyCode::PageDown => Some(CursorMove::PageDown),
        _ => None,
    };
    if let Some(movement) = movement {
        for _ in 0..count {
            app.editor.helix_move(movement, extend);
        }
        return;
    }
    match key.code {
        KeyCode::Esc => {
            app.editor.helix.mode = HelixMode::Normal;
            app.editor.helix.selected_register = None;
        }
        KeyCode::Tab => jump_forward(app, count),
        KeyCode::Char('v') => app.editor.helix.mode = if extend { HelixMode::Normal } else { HelixMode::Select },
        KeyCode::Char('Z') => app.editor.helix.sticky_view = true,
        KeyCode::Char('G') => {
            push_jump(app);
            app.editor.helix_move(if had_count { CursorMove::GoToLine(count) } else { CursorMove::Bottom }, extend);
        }
        KeyCode::Char('i') => {
            begin_insert(app);
            collapse_to(app, false);
        }
        KeyCode::Char('a') => {
            begin_insert(app);
            collapse_to(app, true);
            app.editor.helix.restore_cursor = true;
        }
        KeyCode::Char('I') => {
            begin_insert(app);
            app.editor.helix_move(CursorMove::FirstNonBlank, false);
        }
        KeyCode::Char('A') => {
            begin_insert(app);
            app.editor.helix_move(CursorMove::End, false);
        }
        KeyCode::Char('o') => open_line(app, true),
        KeyCode::Char('O') => open_line(app, false),
        KeyCode::Char('y') => yank(app),
        KeyCode::Char('R') => replace_from_register(app),
        KeyCode::Char('d') => {
            yank(app);
            app.editor.helix_replace(&[String::new()], HelixRangeMode::Selection);
        }
        KeyCode::Char('c') => {
            yank(app);
            app.editor.helix_begin_transaction();
            app.editor.helix_replace(&[String::new()], HelixRangeMode::Selection);
            begin_insert(app);
        }
        KeyCode::Char('p') => paste(app, true),
        KeyCode::Char('P') => paste(app, false),
        KeyCode::Char('>') => indent_lines(app, true, count),
        KeyCode::Char('<') => indent_lines(app, false, count),
        KeyCode::Char('J') => join_lines(app, false),
        KeyCode::Char('&') => align_selections(app),
        KeyCode::Char('_') => trim_selections(app),
        KeyCode::Char('(') => rotate_primary(app, false),
        KeyCode::Char(')') => rotate_primary(app, true),
        KeyCode::Char('*') => search_selection(app, true),
        KeyCode::Char('u') => {
            for _ in 0..count {
                if !app.editor.undo() {
                    break;
                }
            }
        }
        KeyCode::Char('U') => {
            for _ in 0..count {
                if !app.editor.redo() {
                    break;
                }
            }
        }
        KeyCode::Char('Q') => {
            if let Some(register) = app.editor.helix.recording.take() {
                if let Some(keys) = app.editor.helix.macros.get_mut(&register) {
                    if keys.last() == Some(&key) {
                        keys.pop();
                    }
                }
            } else {
                let register = app.editor.helix.take_register();
                app.editor.helix.macros.insert(register, Vec::new());
                app.editor.helix.recording = Some(register);
                app.editor.helix.last_macro = Some(register);
            }
        }
        KeyCode::Char('q') => replay_macro(app, count),
        KeyCode::Char('r') => app.editor.helix.pending = Some('r'),
        KeyCode::Char(prefix @ ('f' | 'F' | 't' | 'T' | 'g' | 'm' | 'z' | ' ' | '"')) => {
            app.editor.helix.pending = Some(prefix);
            app.editor.helix.pending_count = count;
        }
        KeyCode::Char(':') => open_prompt(app, HelixMode::Command),
        KeyCode::Char('/') => open_prompt(app, HelixMode::SearchForward),
        KeyCode::Char('?') => open_prompt(app, HelixMode::SearchBackward),
        KeyCode::Char('s') => open_prompt(app, HelixMode::RegexSelect),
        KeyCode::Char('S') => open_prompt(app, HelixMode::RegexSplit),
        KeyCode::Char('K') => open_prompt(app, HelixMode::RegexKeep(false)),
        KeyCode::Char('n') => search_next(app, false, extend),
        KeyCode::Char('N') => search_next(app, true, extend),
        KeyCode::Char('%') => helix_select_all(app),
        KeyCode::Char(';') => {
            if let Some(set) = selection_set(app) {
                let carets = set.selections.iter().map(|selection| HelixSelection::caret(selection.head)).collect();
                app.editor.helix_set_selections(carets, set.primary);
            }
        }
        KeyCode::Char(',') => {
            if let Some(selection) = app.editor.helix_primary() {
                app.editor.helix_set_selections(vec![selection], 0);
            }
        }
        KeyCode::Char('x') => select_lines(app, LineSelect::Extend, count),
        KeyCode::Char('X') => select_lines(app, LineSelect::Bounds, count),
        KeyCode::Char('C') => copy_selection_to_line(app, true, count),
        KeyCode::Char('~') => transform_selected(app, |text| text.chars().map(|character| if character.is_uppercase() { character.to_lowercase().to_string() } else { character.to_uppercase().to_string() }).collect()),
        KeyCode::Char('`') => transform_selected(app, str::to_lowercase),
        KeyCode::Char('.') => {
            let insert = app.editor.helix.last_insert.clone();
            if !insert.is_empty() {
                app.editor.helix_replace(&[insert], HelixRangeMode::After);
            }
        }
        KeyCode::Char(_) => status(app, "This Helix command is unavailable in Ekphos"),
        _ => {}
    }
}

fn handle_helix_control(app: &mut App, key: crossterm::event::KeyEvent, count: usize, extend: bool) {
    match key.code {
        KeyCode::Char('c') => transform_selected(app, toggle_markdown_comment),
        KeyCode::Char('a') => change_numbers(app, true, count),
        KeyCode::Char('x') => change_numbers(app, false, count),
        KeyCode::Char('s') => push_jump(app),
        KeyCode::Char('o') => {
            if let Some(current) = selection_set(app) {
                if let Some(jump) = app.editor.helix.jump_backward(current, count) {
                    app.editor.helix_set_selections(jump.selections, jump.primary);
                }
            }
        }
        KeyCode::Char('i') => jump_forward(app, count),
        KeyCode::Char('f') => app.editor.helix_move(CursorMove::PageDown, extend),
        KeyCode::Char('b') => app.editor.helix_move(CursorMove::PageUp, extend),
        KeyCode::Char('u') => app.editor.helix_move(CursorMove::HalfPageUp, extend),
        KeyCode::Char('d') => app.editor.helix_move(CursorMove::HalfPageDown, extend),
        _ => {}
    }
}

fn handle_helix_alt(app: &mut App, key: crossterm::event::KeyEvent, count: usize, extend: bool) {
    match key.code {
        KeyCode::Char('C') => copy_selection_to_line(app, false, count),
        KeyCode::Char('c') => {
            app.editor.helix_begin_transaction();
            app.editor.helix_replace(&[String::new()], HelixRangeMode::Selection);
            begin_insert(app);
        }
        KeyCode::Char('s') => split_on_newlines(app),
        KeyCode::Char(';') => flip_selections(app),
        KeyCode::Char(':') => ensure_forward(app),
        KeyCode::Char(',') => remove_primary(app),
        KeyCode::Char('-') => merge_selections(app),
        KeyCode::Char('_') => merge_consecutive_selections(app),
        KeyCode::Char('x') => select_lines(app, LineSelect::Shrink, count),
        KeyCode::Char('K' | 'k') => open_prompt(app, HelixMode::RegexKeep(true)),
        KeyCode::Char('(') => rotate_contents(app, false),
        KeyCode::Char(')') => rotate_contents(app, true),
        KeyCode::Char('*') => search_selection(app, false),
        KeyCode::Char('.') => {
            if let Some((character, forward, till)) = app.editor.helix.last_find {
                find_character(app, character, forward, till, extend, count);
            }
        }
        KeyCode::Char('u') => {
            app.editor.undo();
        }
        KeyCode::Char('U') => {
            app.editor.redo();
        }
        KeyCode::Char('`') => transform_selected(app, str::to_uppercase),
        KeyCode::Char('d') => {
            app.editor.helix_replace(&[String::new()], HelixRangeMode::Selection);
        }
        KeyCode::Char('J') => join_lines(app, true),
        _ => {}
    }
}

fn open_prompt(app: &mut App, mode: HelixMode) {
    app.editor.helix.mode = mode;
    app.editor.helix.prompt.clear();
}

fn jump_forward(app: &mut App, count: usize) {
    if let Some(jump) = app.editor.helix.jump_forward(count) {
        app.editor.helix_set_selections(jump.selections, jump.primary);
    }
}

fn replay_macro(app: &mut App, count: usize) {
    if app.editor.helix.playing_macro {
        return;
    }
    let register = app.editor.helix.selected_register.take().or(app.editor.helix.last_macro);
    let Some(keys) = register.and_then(|register| app.editor.helix.macros.get(&register).cloned()) else { return };
    app.editor.helix.playing_macro = true;
    'replay: for _ in 0..count {
        for key in &keys {
            if app.editor.mode != Mode::Edit {
                break 'replay;
            }
            handle_helix_mode(app, *key);
        }
    }
    app.editor.helix.playing_macro = false;
}

fn open_line(app: &mut App, below: bool) {
    let Some(set) = selection_set(app) else { return };
    let len = app.editor.helix_text_len();
    begin_insert(app);
    let changes = set
        .selections
        .iter()
        .map(|selection| {
            let (first_row, last_row) = line_range(app, *selection, len);
            if below {
                let indent = line_indent(app, last_row);
                let caret = 1 + indent.chars().count();
                let at = app.editor.helix_line_end(last_row);
                HelixChange::new(at, at, format!("\n{indent}")).select(caret, caret)
            } else {
                let indent = line_indent(app, first_row);
                let caret = indent.chars().count();
                let at = app.editor.helix_line_start(first_row);
                HelixChange::new(at, at, format!("{indent}\n")).select(caret, caret)
            }
        })
        .collect();
    app.editor.helix_edit(changes, set.primary);
}

pub(super) fn yank(app: &mut App) {
    let values = app.editor.helix_selected_texts();
    let register = app.editor.helix.take_register();
    if register == '+' || register == '*' {
        if let Err(error) = app.clipboard().set_text(&values.join("\n")) {
            status(app, format!("Clipboard: {error}"));
        }
    }
    app.editor.helix.registers.insert(register, values.clone());
    if register != '"' {
        app.editor.helix.registers.insert('"', values);
    }
}

pub(super) fn paste(app: &mut App, after: bool) {
    let register = app.editor.helix.take_register();
    let values = register_values(app, register);
    if values.is_empty() {
        return;
    }
    let selections = app.editor.helix_selection_set().map_or(0, |set| set.selections.len());
    let values = if values.len() == selections { values } else { vec![values.join("\n")] };
    paste_values(app, &values, after);
}

fn paste_values(app: &mut App, values: &[String], after: bool) -> bool {
    let Some(set) = selection_set(app) else { return false };
    let linewise = values.iter().any(|value| value.ends_with('\n'));
    let len = app.editor.helix_text_len();
    let changes = set
        .selections
        .iter()
        .enumerate()
        .map(|(index, selection)| {
            let value = values.get(index).or_else(|| values.last()).cloned().unwrap_or_default();
            let (start, end) = selection.range(len);
            let (first_row, last_row) = line_range(app, *selection, len);
            let (at, text, skip) = match (linewise, after) {
                (true, true) if last_row + 1 < app.editor.line_count() => (app.editor.helix_line_start(last_row + 1), value, 0),
                (true, true) => (len, format!("\n{}", value.strip_suffix('\n').unwrap_or(&value)), 1),
                (true, false) => (app.editor.helix_line_start(first_row), value, 0),
                (false, true) => (end, value, 0),
                (false, false) => (start, value, 0),
            };
            let selected = HelixSelection::spanning(skip, text.chars().count(), false);
            HelixChange::new(at, at, text).select(selected.anchor, selected.head)
        })
        .collect();
    app.editor.helix_edit(changes, set.primary)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineSelect {
    Extend,
    Bounds,
    Shrink,
}

fn select_lines(app: &mut App, kind: LineSelect, count: usize) {
    let Some(set) = selection_set(app) else { return };
    let len = app.editor.helix_text_len();
    let last_row = app.editor.line_count().saturating_sub(1);
    let selections = set
        .selections
        .iter()
        .map(|selection| {
            let (from, to) = selection.range(len);
            let (start_row, end_row) = line_range(app, *selection, len);
            let start = app.editor.helix_line_start(start_row);
            let end = line_bound(app, end_row);
            let reversed = selection.is_reversed();
            match kind {
                LineSelect::Extend => {
                    let full = from == start && to == end && to > from;
                    let target = (end_row + count - usize::from(!full)).min(last_row);
                    let end = line_bound(app, target);
                    if end > start {
                        HelixSelection::spanning(start, end, false)
                    } else {
                        HelixSelection::caret(start.saturating_sub(1))
                    }
                }
                LineSelect::Bounds if end > start => HelixSelection::spanning(start, end, reversed),
                LineSelect::Shrink if start_row != end_row => {
                    let start = if start == from { start } else { app.editor.helix_line_start(start_row + 1) };
                    let end = if end == to { end } else { app.editor.helix_line_start(end_row) };
                    if end > start {
                        HelixSelection::spanning(start, end, reversed)
                    } else {
                        *selection
                    }
                }
                _ => *selection,
            }
        })
        .collect();
    app.editor.helix_set_selections(selections, set.primary);
}

fn copy_selection_to_line(app: &mut App, below: bool, count: usize) {
    let Some(set) = selection_set(app) else { return };
    let line_count = app.editor.line_count();
    let line_len = |row: usize| app.editor.line(row).map_or(0, |line| line.chars().count());
    let mut selections = set.selections.clone();
    let mut primary = set.primary;
    for (index, selection) in set.selections.iter().enumerate() {
        let anchor = app.editor.helix_position(selection.anchor);
        let head = app.editor.helix_position(selection.head);
        let height = anchor.row.abs_diff(head.row) + 1;
        let mut added = 0;
        let mut step = 1;
        while added < count {
            let offset = step * height;
            let rows = if below { Some((anchor.row + offset, head.row + offset)) } else { anchor.row.checked_sub(offset).zip(head.row.checked_sub(offset)) };
            let Some((anchor_row, head_row)) = rows.filter(|(anchor_row, head_row)| *anchor_row < line_count && *head_row < line_count) else { break };
            if line_len(anchor_row) >= anchor.col && line_len(head_row) >= head.col {
                if index == set.primary {
                    primary = selections.len();
                }
                selections.push(HelixSelection { anchor: app.editor.helix_offset(Position::new(anchor_row, anchor.col)), head: app.editor.helix_offset(Position::new(head_row, head.col)) });
                added += 1;
            }
            step += 1;
        }
    }
    app.editor.helix_set_selections(selections, primary);
}

fn transform_selected(app: &mut App, mut transform: impl FnMut(&str) -> String) {
    let replacements = app.editor.helix_selected_texts().iter().map(|text| transform(text)).collect::<Vec<_>>();
    app.editor.helix_replace(&replacements, HelixRangeMode::Selection);
}

fn number_around(chars: &[char], cursor: usize) -> Option<(usize, usize)> {
    let digit_at = |index: usize| chars.get(index).is_some_and(char::is_ascii_digit);
    let mut start = if digit_at(cursor) {
        cursor
    } else if chars.get(cursor) == Some(&'-') && digit_at(cursor + 1) {
        cursor + 1
    } else {
        return None;
    };
    while start > 0 && digit_at(start - 1) {
        start -= 1;
    }
    let mut end = start;
    while digit_at(end) {
        end += 1;
    }
    if start > 0 && chars[start - 1] == '-' {
        start -= 1;
    }
    Some((start, end))
}

fn change_numbers(app: &mut App, increase: bool, count: usize) {
    let Some(set) = selection_set(app) else { return };
    let chars = app.editor.helix_chars();
    let len = chars.len();
    let delta = i64::try_from(count).unwrap_or(i64::MAX);
    let mut previous_end = 0;
    let changes = set
        .selections
        .iter()
        .map(|selection| {
            let (start, end) = selection.range(len);
            let number = if end <= start + 1 { number_around(&chars, start) } else { Some((start, end)) };
            let replacement = number.filter(|(number_start, _)| *number_start >= previous_end).and_then(|(number_start, number_end)| {
                let value: i64 = chars[number_start..number_end].iter().collect::<String>().parse().ok()?;
                let value = if increase { value.saturating_add(delta) } else { value.saturating_sub(delta) };
                Some((number_start, number_end, value.to_string()))
            });
            match replacement {
                Some((number_start, number_end, text)) => {
                    previous_end = number_end;
                    let selected = HelixSelection::spanning(0, text.chars().count(), selection.is_reversed());
                    HelixChange::new(number_start, number_end, text).select(selected.anchor, selected.head)
                }
                None => HelixChange::new(start, start, "").select(selection.anchor - start, selection.head - start),
            }
        })
        .collect();
    app.editor.helix_edit(changes, set.primary);
}

fn replace_from_register(app: &mut App) {
    let register = app.editor.helix.take_register();
    let values = register_values(app, register);
    if !values.is_empty() {
        app.editor.helix_replace(&values, HelixRangeMode::Selection);
    }
}

fn split_on_newlines(app: &mut App) {
    let Some(set) = selection_set(app) else { return };
    let chars = app.editor.helix_chars();
    let mut selections = Vec::new();
    for selection in set.selections {
        let (start, end) = selection.range(chars.len());
        let mut part_start = start;
        for (offset, character) in chars.iter().enumerate().take(end).skip(start) {
            if *character == '\n' {
                if part_start < offset {
                    selections.push(HelixSelection { anchor: part_start, head: offset - 1 });
                }
                part_start = offset + 1;
            }
        }
        if part_start < end {
            selections.push(HelixSelection { anchor: part_start, head: end - 1 });
        }
    }
    if !selections.is_empty() {
        app.editor.helix_set_selections(selections, 0);
    }
}

fn flip_selections(app: &mut App) {
    let Some(set) = selection_set(app) else { return };
    let selections = set.selections.into_iter().map(|selection| HelixSelection { anchor: selection.head, head: selection.anchor }).collect();
    app.editor.helix_set_selections(selections, set.primary);
}

fn ensure_forward(app: &mut App) {
    let Some(set) = selection_set(app) else { return };
    let selections = set.selections.into_iter().map(|selection| HelixSelection { anchor: selection.anchor.min(selection.head), head: selection.anchor.max(selection.head) }).collect();
    app.editor.helix_set_selections(selections, set.primary);
}

fn remove_primary(app: &mut App) {
    let Some(mut set) = selection_set(app) else { return };
    if set.selections.len() <= 1 {
        return;
    }
    set.selections.remove(set.primary);
    app.editor.helix_set_selections(set.selections, set.primary.saturating_sub(1));
}

fn merge_selections(app: &mut App) {
    let Some(set) = selection_set(app) else { return };
    let len = app.editor.helix_text_len();
    let start = set.selections.iter().map(|selection| selection.range(len).0).min().unwrap_or(0);
    let end = set.selections.iter().map(|selection| selection.range(len).1).max().unwrap_or(start);
    app.editor.helix_set_selections(vec![HelixSelection::spanning(start, end, false)], 0);
}

fn merge_consecutive_selections(app: &mut App) {
    let Some(set) = selection_set(app) else { return };
    let len = app.editor.helix_text_len();
    let mut merged: Vec<HelixSelection> = Vec::with_capacity(set.selections.len());
    let mut primary = 0;
    for (index, selection) in set.selections.iter().enumerate() {
        let (start, end) = selection.range(len);
        if let Some(last) = merged.last_mut() {
            let (last_start, last_end) = last.range(len);
            if start <= last_end {
                *last = HelixSelection::spanning(last_start, last_end.max(end), last.is_reversed());
                if index == set.primary {
                    primary = merged.len() - 1;
                }
                continue;
            }
        }
        if index == set.primary {
            primary = merged.len();
        }
        merged.push(*selection);
    }
    app.editor.helix_set_selections(merged, primary);
}

fn rotate_primary(app: &mut App, forward: bool) {
    let Some(set) = selection_set(app) else { return };
    let len = set.selections.len();
    if len < 2 {
        return;
    }
    let primary = if forward { (set.primary + 1) % len } else { (set.primary + len - 1) % len };
    app.editor.helix_set_selections(set.selections, primary);
}

fn rotate_contents(app: &mut App, forward: bool) {
    let mut texts = app.editor.helix_selected_texts();
    if texts.len() < 2 {
        return;
    }
    if forward {
        texts.rotate_right(1);
    } else {
        texts.rotate_left(1);
    }
    app.editor.helix_replace(&texts, HelixRangeMode::Selection);
}

fn align_selections(app: &mut App) {
    let Some(set) = selection_set(app) else { return };
    let len = app.editor.helix_text_len();
    let mut columns: Vec<Vec<(usize, usize, usize)>> = Vec::new();
    let mut last_row = None;
    let mut column = 0;
    for selection in &set.selections {
        let anchor = app.editor.helix_position(selection.anchor);
        let head = app.editor.helix_position(selection.head);
        if anchor.row != head.row {
            status(app, "Align cannot work with multi-line selections");
            return;
        }
        column = if last_row == Some(head.row) { column + 1 } else { 0 };
        if column == columns.len() {
            columns.push(Vec::new());
        }
        columns[column].push((selection.range(len).0, head.col, head.row));
        last_row = Some(head.row);
    }
    let mut offsets: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut changes = Vec::new();
    for column in columns {
        let target = column.iter().map(|(_, col, row)| col + offsets.get(row).copied().unwrap_or(0)).max().unwrap_or(0);
        for (at, col, row) in column {
            let offset = offsets.entry(row).or_insert(0);
            let padding = target - (col + *offset);
            if padding > 0 {
                *offset += padding;
                changes.push(HelixChange::new(at, at, " ".repeat(padding)));
            }
        }
    }
    app.editor.helix_edit(changes, set.primary);
}

fn trim_selections(app: &mut App) {
    let Some(set) = selection_set(app) else { return };
    let chars = app.editor.helix_chars();
    let mut trimmed = Vec::new();
    let mut primary = None;
    for (index, selection) in set.selections.iter().enumerate() {
        let (mut start, mut end) = selection.range(chars.len());
        while start < end && chars[start].is_whitespace() {
            start += 1;
        }
        while end > start && chars[end - 1].is_whitespace() {
            end -= 1;
        }
        if start < end {
            if index == set.primary {
                primary = Some(trimmed.len());
            }
            trimmed.push(HelixSelection::spanning(start, end, selection.is_reversed()));
        }
    }
    if trimmed.is_empty() {
        let head = set.selections[set.primary].head;
        app.editor.helix_set_selections(vec![HelixSelection::caret(head)], 0);
    } else {
        let primary = primary.unwrap_or(trimmed.len() - 1);
        app.editor.helix_set_selections(trimmed, primary);
    }
}

fn toggle_markdown_comment(text: &str) -> String {
    let (body, newline) = if let Some(body) = text.strip_suffix('\n') { (body, "\n") } else { (text, "") };
    if let Some(inner) = body.strip_prefix("<!-- ").and_then(|body| body.strip_suffix(" -->")) {
        format!("{inner}{newline}")
    } else {
        format!("<!-- {body} -->{newline}")
    }
}

fn indent_lines(app: &mut App, indent: bool, count: usize) {
    let Some(set) = selection_set(app) else { return };
    let len = app.editor.helix_text_len();
    let mut rows: Vec<usize> = set
        .selections
        .iter()
        .flat_map(|selection| {
            let (first, last) = line_range(app, *selection, len);
            first..=last
        })
        .collect();
    rows.sort_unstable();
    rows.dedup();
    let tab_width = usize::from(app.state.config.editor.tab_width.max(1));
    let changes = rows
        .into_iter()
        .filter_map(|row| {
            let line = app.editor.line(row)?;
            let start = app.editor.helix_line_start(row);
            if indent {
                return (!line.trim().is_empty()).then(|| HelixChange::new(start, start, "\t".repeat(count)));
            }
            let chars: Vec<char> = line.chars().collect();
            let mut removed = 0;
            for _ in 0..count {
                if chars.get(removed) == Some(&'\t') {
                    removed += 1;
                } else {
                    let spaces = chars.iter().skip(removed).take(tab_width).take_while(|character| **character == ' ').count();
                    if spaces == 0 {
                        break;
                    }
                    removed += spaces;
                }
            }
            (removed > 0).then(|| HelixChange::new(start, start + removed, ""))
        })
        .collect();
    app.editor.helix_edit(changes, set.primary);
}

fn join_lines(app: &mut App, select_space: bool) {
    let Some(set) = selection_set(app) else { return };
    let len = app.editor.helix_text_len();
    let last_row = app.editor.line_count().saturating_sub(1);
    let mut rows: Vec<usize> = set
        .selections
        .iter()
        .flat_map(|selection| {
            let (first, last) = line_range(app, *selection, len);
            let last = if first == last { (last + 1).min(last_row) } else { last };
            first..last
        })
        .collect();
    rows.sort_unstable();
    rows.dedup();
    let changes: Vec<HelixChange> = rows
        .into_iter()
        .map(|row| {
            let start = app.editor.helix_line_end(row);
            let next_start = app.editor.helix_line_start(row + 1);
            let indent = line_indent(app, row + 1).chars().count();
            let blank = next_start + indent == app.editor.helix_line_end(row + 1);
            let change = HelixChange::new(start, next_start + indent, if blank { "" } else { " " });
            if select_space && !blank {
                change.select(0, 0)
            } else {
                change
            }
        })
        .collect();
    app.editor.helix_edit(changes, set.primary);
}

fn search_selection(app: &mut App, word_boundaries: bool) {
    let mut patterns: Vec<String> = Vec::new();
    for text in app.editor.helix_selected_texts() {
        if text.is_empty() {
            continue;
        }
        let escaped = regex::escape(&text);
        let pattern = if word_boundaries && text.chars().all(is_word_char) { format!(r"\b{escaped}\b") } else { escaped };
        if !patterns.contains(&pattern) {
            patterns.push(pattern);
        }
    }
    if patterns.is_empty() {
        return;
    }
    let pattern = patterns.join("|");
    status(app, format!("Search: {pattern}"));
    app.editor.helix.search_pattern = pattern;
}

fn surround_pair(character: char) -> (char, char) {
    match character {
        '(' | ')' => ('(', ')'),
        '[' | ']' => ('[', ']'),
        '{' | '}' => ('{', '}'),
        '<' | '>' => ('<', '>'),
        other => (other, other),
    }
}

fn surround_add(app: &mut App, character: char) {
    let (open, close) = surround_pair(character);
    transform_selected(app, |text| format!("{open}{text}{close}"));
}

fn surround_change(app: &mut App, from: char, to: Option<char>) {
    let Some((_, object)) = TextObject::parse('a', from).filter(|(_, object)| object.delimiters().is_some()) else {
        status(app, "Unknown surround pair");
        return;
    };
    let Some(set) = selection_set(app) else { return };
    let snapshot = app.editor.snapshot();
    let mut selections = Vec::new();
    for selection in set.selections {
        let pos = app.editor.helix_position(selection.head);
        let Some((start, end)) = object.find_bounds_snapshot(TextObjectScope::Around, &snapshot, pos) else { continue };
        let start = app.editor.helix_offset(start);
        let end = app.editor.helix_offset(end);
        if end > start + 1 {
            selections.push(HelixSelection::spanning(start, end, false));
        }
    }
    if selections.is_empty() {
        status(app, "No surround pair found");
        return;
    }
    app.editor.helix_set_selections(selections, 0);
    let new_pair = to.map(surround_pair);
    transform_selected(app, |text| {
        let mut chars = text.chars();
        let _ = chars.next();
        let _ = chars.next_back();
        let inner: String = chars.collect();
        if let Some((open, close)) = new_pair {
            format!("{open}{inner}{close}")
        } else {
            inner
        }
    });
}

fn select_text_object(app: &mut App, around: bool, character: char) {
    let Some((scope, object)) = TextObject::parse(if around { 'a' } else { 'i' }, character) else {
        status(app, "Unknown text object");
        return;
    };
    let Some(set) = selection_set(app) else { return };
    let snapshot = app.editor.snapshot();
    let selections = set
        .selections
        .iter()
        .map(|selection| {
            let pos = app.editor.helix_position(selection.head);
            match object.find_bounds_snapshot(scope, &snapshot, pos) {
                Some((start, end)) if end > start => HelixSelection::spanning(app.editor.helix_offset(start), app.editor.helix_offset(end), false),
                _ => *selection,
            }
        })
        .collect();
    app.editor.helix_set_selections(selections, set.primary);
}

fn handle_view_key(app: &mut App, character: char) {
    match character {
        'z' | 'c' => app.editor.center_cursor(),
        't' => {
            let row = app.editor.cursor().0;
            app.editor.set_scroll_offset(row);
            app.editor.editor_scroll_top = app.editor.scroll_offset();
        }
        'b' => {
            let row = app.editor.cursor().0;
            let view_height = app.editor.editor_view_height;
            app.editor.set_scroll_offset(row.saturating_sub(view_height.saturating_sub(2)));
            app.editor.editor_scroll_top = app.editor.scroll_offset();
        }
        'j' | 'k' => {
            let delta = if character == 'j' { 1 } else { -1 };
            let row = app.editor.visible_row_at_offset(app.editor.scroll_offset(), delta);
            app.editor.set_scroll_offset(row);
            app.editor.editor_scroll_top = app.editor.scroll_offset();
            constrain_cursor_to_viewport(app);
        }
        _ => status(app, "Unavailable view command"),
    }
}

fn prefix_character(prefix: char, key: crossterm::event::KeyEvent) -> Option<char> {
    let accepts_whitespace = matches!(prefix, 'r' | 'f' | 'F' | 't' | 'T');
    match key.code {
        KeyCode::Char(character) => Some(character),
        KeyCode::Enter if accepts_whitespace => Some('\n'),
        KeyCode::Tab if accepts_whitespace => Some('\t'),
        _ => None,
    }
}

fn handle_helix_prefix(app: &mut App, prefix: char, key: crossterm::event::KeyEvent) {
    let Some(character) = prefix_character(prefix, key) else {
        app.editor.helix.surround_from = None;
        return;
    };
    let extend = app.editor.helix.mode == HelixMode::Select;
    let pending_count = app.editor.helix.pending_count.max(1);
    match prefix {
        '"' => app.editor.helix.selected_register = Some(character),
        'g' => match character {
            'g' => {
                push_jump(app);
                app.editor.helix_move(if pending_count == 1 { CursorMove::Top } else { CursorMove::GoToLine(pending_count) }, extend);
            }
            'e' => {
                push_jump(app);
                app.editor.helix_move(CursorMove::Bottom, extend);
            }
            'h' => app.editor.helix_move(CursorMove::Head, extend),
            'l' => app.editor.helix_move(CursorMove::End, extend),
            's' => app.editor.helix_move(CursorMove::FirstNonBlank, extend),
            't' => app.editor.helix_move(CursorMove::ScreenTop, extend),
            'c' => app.editor.helix_move(CursorMove::ScreenMiddle, extend),
            'b' => app.editor.helix_move(CursorMove::ScreenBottom, extend),
            'j' => app.editor.helix_move(CursorMove::Down, extend),
            'k' => app.editor.helix_move(CursorMove::Up, extend),
            '|' => app.editor.helix_move(CursorMove::GoToColumn(pending_count), extend),
            'f' => follow_link(app),
            _ => status(app, "Unavailable goto command"),
        },
        'z' => handle_view_key(app, character),
        ' ' => match character {
            'f' | '/' => {
                if app.has_unsaved_changes() {
                    status(app, "Save with :w before opening the note picker");
                } else {
                    app.editor.helix_end();
                    app.cancel_edit();
                    app.open_search_picker();
                    if character == '/' {
                        app.toggle_search_picker_mode();
                    }
                }
            }
            '?' => app.state.dialog = DialogState::Help,
            'c' => transform_selected(app, toggle_markdown_comment),
            'y' => {
                let text = app.editor.helix_selected_texts().join("\n");
                if let Err(error) = app.clipboard().set_text(&text) {
                    status(app, format!("Clipboard: {error}"));
                }
            }
            'p' | 'P' => {
                app.editor.helix.selected_register = Some('+');
                paste(app, character == 'p');
            }
            'R' => {
                app.editor.helix.selected_register = Some('+');
                replace_from_register(app);
            }
            _ => status(app, "This Helix space command is unavailable in Ekphos"),
        },
        'm' => match character {
            'm' => app.editor.helix_move(CursorMove::MatchingBracket, extend),
            'a' => app.editor.helix.pending = Some('A'),
            'i' => app.editor.helix.pending = Some('I'),
            's' => app.editor.helix.pending = Some('s'),
            'r' => app.editor.helix.pending = Some('1'),
            'd' => app.editor.helix.pending = Some('d'),
            _ => status(app, "This Helix match command is unavailable in Ekphos"),
        },
        's' => surround_add(app, character),
        'd' => surround_change(app, character, None),
        '1' => {
            app.editor.helix.surround_from = Some(character);
            app.editor.helix.pending = Some('2');
        }
        '2' => {
            if let Some(from) = app.editor.helix.surround_from.take() {
                surround_change(app, from, Some(character));
            }
        }
        'A' | 'I' => select_text_object(app, prefix == 'A', character),
        'r' => {
            let texts = app.editor.helix_selected_texts().into_iter().map(|selected| selected.chars().map(|original| if original == '\n' { original } else { character }).collect()).collect::<Vec<String>>();
            app.editor.helix_replace(&texts, HelixRangeMode::Selection);
        }
        'f' | 'F' | 't' | 'T' => {
            let forward = prefix == 'f' || prefix == 't';
            let till = prefix == 't' || prefix == 'T';
            app.editor.helix.last_find = Some((character, forward, till));
            find_character(app, character, forward, till, extend, pending_count);
        }
        _ => {}
    }
}

fn find_character(app: &mut App, character: char, forward: bool, till: bool, extend: bool, count: usize) {
    let chars = app.editor.helix_chars();
    let Some(set) = selection_set(app) else { return };
    let selections = set
        .selections
        .iter()
        .map(|selection| {
            let cursor = selection.head.min(chars.len());
            let found = if forward {
                let skip_adjacent = till && chars.get(cursor + 1) == Some(&character);
                let from = (cursor + 1 + usize::from(skip_adjacent)).min(chars.len());
                (from..chars.len()).filter(|index| chars[*index] == character).nth(count - 1).map(|index| if till { index - 1 } else { index })
            } else {
                let skip_adjacent = till && cursor > 0 && chars.get(cursor - 1) == Some(&character);
                let until = cursor.saturating_sub(usize::from(skip_adjacent));
                (0..until).rev().filter(|index| chars[*index] == character).nth(count - 1).map(|index| if till { index + 1 } else { index })
            };
            match found {
                Some(head) if extend => HelixSelection { head, ..*selection },
                Some(head) => HelixSelection { anchor: cursor, head },
                None => *selection,
            }
        })
        .collect();
    app.editor.helix_set_selections(selections, set.primary);
}

enum LinkTarget {
    Wiki { target: String, heading: Option<String> },
    Url(String),
    Image(String),
}

fn link_at_primary(app: &App) -> Option<LinkTarget> {
    let pos = app.editor.helix_position(app.editor.helix_primary()?.head);
    let line = app.editor.line(pos.row)?;
    let byte = line.char_indices().nth(pos.col).map_or(line.len(), |(index, _)| index);
    if let Some(link) = crate::core::markdown::wiki_links(line).into_iter().find(|link| link.range.contains(&byte)) {
        return Some(LinkTarget::Wiki { target: link.target.to_string(), heading: link.heading.map(str::to_string) });
    }
    if let Some(captures) = MARKDOWN_LINK.captures_iter(line).find(|captures| captures.get(0).is_some_and(|whole| whole.range().contains(&byte))) {
        let url = captures.get(2)?.as_str().to_string();
        return Some(if captures.get(1).is_some_and(|bang| !bang.as_str().is_empty()) { LinkTarget::Image(url) } else { LinkTarget::Url(url) });
    }
    BARE_URL.find_iter(line).find(|url| url.range().contains(&byte)).map(|url| LinkTarget::Url(url.as_str().trim_end_matches(['.', ',', ';', ':']).to_string()))
}

fn follow_link(app: &mut App) {
    let Some(link) = link_at_primary(app) else {
        status(app, "No link at selection");
        return;
    };
    match link {
        LinkTarget::Url(url) if url.starts_with("http://") || url.starts_with("https://") => app.open_link(&url),
        LinkTarget::Image(path) => app.open_path_or_url(&path),
        link => {
            if app.has_unsaved_changes() {
                status(app, "Save with :w before following a link");
                return;
            }
            app.editor.helix_end();
            app.cancel_edit();
            match link {
                LinkTarget::Wiki { target, heading } if app.resolve_wiki_link(&target).is_some() => {
                    app.navigate_to_wiki_link_with_heading(&target, heading.as_deref());
                }
                LinkTarget::Wiki { target, .. } => {
                    app.editor.pending_wiki_target = Some(target);
                    app.state.dialog = DialogState::CreateWikiNote;
                }
                LinkTarget::Url(url) => app.open_link(&url),
                LinkTarget::Image(_) => {}
            }
        }
    }
}

fn handle_helix_prompt(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.editor.helix.mode = HelixMode::Normal;
            app.editor.helix.prompt.clear();
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.editor.helix.mode = HelixMode::Normal;
            app.editor.helix.prompt.clear();
        }
        KeyCode::Backspace => {
            if app.editor.helix.prompt.pop().is_none() {
                app.editor.helix.mode = HelixMode::Normal;
            }
        }
        KeyCode::Enter => {
            let mode = app.editor.helix.mode;
            let prompt = std::mem::take(&mut app.editor.helix.prompt);
            app.editor.helix.mode = HelixMode::Normal;
            match mode {
                HelixMode::Command => execute_helix_command(app, &prompt),
                HelixMode::SearchForward | HelixMode::SearchBackward => {
                    if !prompt.is_empty() {
                        app.editor.helix.search_pattern = prompt;
                    }
                    search_next(app, mode == HelixMode::SearchBackward, false);
                }
                HelixMode::RegexSelect | HelixMode::RegexSplit | HelixMode::RegexKeep(_) => apply_regex_selection(app, &prompt, mode),
                _ => {}
            }
        }
        KeyCode::Char(character) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) => app.editor.helix.prompt.push(character),
        _ => {}
    }
}

fn execute_helix_command(app: &mut App, input: &str) {
    match input.trim() {
        "w" | "write" | "w!" | "write!" => {
            app.save_edit_in_place();
        }
        "wq" | "x" | "wq!" | "x!" | "write-quit" | "write-quit!" => {
            app.save_edit();
        }
        "q" | "quit" => {
            if app.has_unsaved_changes() {
                app.state.dialog = DialogState::UnsavedChanges;
            } else {
                app.editor.helix_end();
                app.cancel_edit();
            }
        }
        "q!" | "quit!" => {
            app.editor.helix_end();
            app.cancel_edit();
        }
        command => {
            if let Ok(line) = command.parse::<usize>() {
                push_jump(app);
                app.editor.helix_move(CursorMove::GoToLine(line), false);
            } else {
                status(app, format!("Unknown command: {command}"));
            }
        }
    }
}

fn build_regex(app: &mut App, pattern: &str) -> Option<Regex> {
    let case_insensitive = !pattern.chars().any(char::is_uppercase);
    match RegexBuilder::new(pattern).case_insensitive(case_insensitive).multi_line(true).build() {
        Ok(regex) => Some(regex),
        Err(error) => {
            status(app, format!("Regex: {error}"));
            None
        }
    }
}

fn char_matches(regex: &Regex, text: &str, base: usize) -> Vec<(usize, usize)> {
    let mut matches = Vec::new();
    let mut byte = 0;
    let mut offset = base;
    for found in regex.find_iter(text) {
        offset += text[byte..found.start()].chars().count();
        let start = offset;
        offset += found.as_str().chars().count();
        byte = found.end();
        matches.push((start, offset));
    }
    matches
}

fn search_next(app: &mut App, reverse: bool, extend: bool) {
    let pattern = app.editor.helix.search_pattern.clone();
    if pattern.is_empty() {
        return;
    }
    let Some(regex) = build_regex(app, &pattern) else { return };
    let text = app.editor.text();
    let matches: Vec<(usize, usize)> = char_matches(&regex, &text, 0).into_iter().filter(|(start, end)| start < end).collect();
    if matches.is_empty() {
        status(app, "No matches");
        return;
    }
    let Some(set) = selection_set(app) else { return };
    let (from, to) = set.selections[set.primary].range(app.editor.helix_text_len());
    let next = if reverse { matches.iter().rev().find(|(_, end)| *end <= from) } else { matches.iter().find(|(start, _)| *start >= to) };
    let wrapped = next.is_none();
    let Some(&(start, end)) = next.or(if reverse { matches.last() } else { matches.first() }) else { return };
    push_jump(app);
    let found = HelixSelection::spanning(start, end, false);
    if extend {
        let mut selections = set.selections;
        selections.push(found);
        let primary = selections.len() - 1;
        app.editor.helix_set_selections(selections, primary);
    } else {
        app.editor.helix_set_selections(vec![found], 0);
    }
    if wrapped {
        status(app, "Wrapped around document");
    }
}

fn apply_regex_selection(app: &mut App, pattern: &str, mode: HelixMode) {
    let Some(regex) = build_regex(app, pattern) else { return };
    let chars = app.editor.helix_chars();
    let Some(set) = selection_set(app) else { return };
    let mut selections = Vec::new();
    for selection in set.selections {
        let (start, end) = selection.range(chars.len());
        let selected: String = chars[start..end].iter().collect();
        match mode {
            HelixMode::RegexKeep(negative) => {
                if regex.is_match(&selected) != negative {
                    selections.push(selection);
                }
            }
            HelixMode::RegexSelect => {
                selections.extend(char_matches(&regex, &selected, start).into_iter().filter(|(match_start, match_end)| match_start < match_end).map(|(match_start, match_end)| HelixSelection::spanning(match_start, match_end, false)));
            }
            HelixMode::RegexSplit => {
                let mut cursor = start;
                for (match_start, match_end) in char_matches(&regex, &selected, start) {
                    if match_start > cursor {
                        selections.push(HelixSelection::spanning(cursor, match_start, false));
                    }
                    cursor = match_end;
                }
                if cursor < end {
                    selections.push(HelixSelection::spanning(cursor, end, false));
                }
            }
            _ => {}
        }
    }
    if selections.is_empty() {
        status(app, "No selections matched");
        return;
    }
    app.editor.helix_set_selections(selections, 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppDependencies;
    use crate::clipboard::MemoryClipboard;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    struct HelixApp {
        app: App,
        root: PathBuf,
        note: PathBuf,
        clipboard: Arc<MemoryClipboard>,
    }

    impl HelixApp {
        fn new() -> Self {
            Self::with_text("hello world")
        }

        fn with_text(text: &str) -> Self {
            let id = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("ekphos-helix-{}-{id}", std::process::id()));
            let vault = root.join("vault");
            fs::create_dir_all(&vault).unwrap();
            let note = vault.join("fixture.md");
            fs::write(&note, text).unwrap();
            let config = Config { general: crate::config::GeneralConfig { welcome_shown: false, check_updates: false, ..Default::default() }, editor: crate::config::EditorConfig { mode: EditingMode::Helix, ..Default::default() }, ..Default::default() };
            let clipboard = Arc::new(MemoryClipboard::default());
            let mut dependencies = AppDependencies::headless(root.join("config"), root.join("cache"));
            dependencies.clipboard = clipboard.clone();
            let mut app = App::new_injected(config, vault, None, dependencies);
            app.state.show_welcome = false;
            app.state.dialog = DialogState::None;
            app.enter_edit_mode();
            app.editor.helix_set_selections(vec![HelixSelection::caret(0)], 0);
            Self { app, root, note, clipboard }
        }

        fn keys(&mut self, text: &str) {
            type_keys(&mut self.app, text);
        }

        fn press(&mut self, code: KeyCode, modifiers: KeyModifiers) {
            handle_helix_mode(&mut self.app, key(code, modifiers));
        }

        fn select(&mut self, selections: Vec<HelixSelection>, primary: usize) {
            self.app.editor.helix_set_selections(selections, primary);
        }

        fn text(&self) -> String {
            self.app.editor.text()
        }

        fn selected(&self) -> Vec<String> {
            self.app.editor.helix_selected_texts()
        }
    }

    impl Drop for HelixApp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn key(code: KeyCode, modifiers: KeyModifiers) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(code, modifiers)
    }

    fn type_keys(app: &mut App, text: &str) {
        for character in text.chars() {
            handle_helix_mode(app, key(KeyCode::Char(character), KeyModifiers::NONE));
        }
    }

    #[test]
    fn multi_cursor_insert_is_one_undo_step() {
        let mut fixture = HelixApp::new();
        fixture.app.editor.helix_set_selections(vec![HelixSelection::caret(0), HelixSelection::caret(6)], 0);
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char('i'), KeyModifiers::NONE));
        type_keys(&mut fixture.app, "XY");
        handle_helix_mode(&mut fixture.app, key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(fixture.app.editor.text(), "XYhello XYworld");
        assert!(fixture.app.editor.undo());
        assert_eq!(fixture.app.editor.text(), "hello world");
        assert!(!fixture.app.editor.undo());
    }

    #[test]
    fn native_ctrl_s_and_command_save() {
        let mut fixture = HelixApp::new();
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(fixture.app.editor.helix.jump_list.len(), 1);
        assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "hello world");
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char('i'), KeyModifiers::NONE));
        type_keys(&mut fixture.app, "X");
        handle_helix_mode(&mut fixture.app, key(KeyCode::Esc, KeyModifiers::NONE));
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char(':'), KeyModifiers::NONE));
        type_keys(&mut fixture.app, "w");
        handle_helix_mode(&mut fixture.app, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(fixture.app.editor.mode, Mode::Edit);
        assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "Xhello world");
    }

    #[test]
    fn failed_write_and_quit_keeps_edit_session_usable() {
        let mut fixture = HelixApp::new();
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char('i'), KeyModifiers::NONE));
        type_keys(&mut fixture.app, "X");
        handle_helix_mode(&mut fixture.app, key(KeyCode::Esc, KeyModifiers::NONE));
        fs::remove_file(&fixture.note).unwrap();
        fs::create_dir(&fixture.note).unwrap();
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char(':'), KeyModifiers::NONE));
        type_keys(&mut fixture.app, "wq");
        handle_helix_mode(&mut fixture.app, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(fixture.app.editor.mode, Mode::Edit);
        assert!(fixture.app.editor.helix_active());
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char('i'), KeyModifiers::NONE));
        type_keys(&mut fixture.app, "Y");
        handle_helix_mode(&mut fixture.app, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(fixture.app.editor.text().contains('Y'));
    }

    #[test]
    fn mode_picker_preserves_unsaved_buffer() {
        let mut fixture = HelixApp::new();
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char('i'), KeyModifiers::NONE));
        type_keys(&mut fixture.app, "X");
        handle_edit_mode(&mut fixture.app, key(KeyCode::F(6), KeyModifiers::NONE));
        assert_eq!(fixture.app.state.dialog, DialogState::EditorModeSelector);
        assert_eq!(fixture.app.state.editor_mode_selected, EditingMode::Helix);
        handle_editor_mode_selector(&mut fixture.app, key(KeyCode::Down, KeyModifiers::NONE));
        handle_editor_mode_selector(&mut fixture.app, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(fixture.app.state.config.editor.mode, EditingMode::Standard);
        assert_eq!(Config::load_from_dir(&fixture.root.join("config")).editor.mode, EditingMode::Standard);
        assert_eq!(fixture.app.editor.text(), "Xhello world");
        assert!(!fixture.app.editor.helix_active());
        assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "hello world");
    }

    #[test]
    fn cancelling_mode_picker_keeps_insert_session() {
        let mut fixture = HelixApp::new();
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char('i'), KeyModifiers::NONE));
        handle_edit_mode(&mut fixture.app, key(KeyCode::F(6), KeyModifiers::NONE));
        handle_editor_mode_selector(&mut fixture.app, key(KeyCode::Down, KeyModifiers::NONE));
        handle_editor_mode_selector(&mut fixture.app, key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(fixture.app.state.config.editor.mode, EditingMode::Helix);
        assert_eq!(fixture.app.editor.helix.mode, HelixMode::Insert);
        type_keys(&mut fixture.app, "ok");
        handle_helix_mode(&mut fixture.app, key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(fixture.app.editor.text(), "okhello world");
    }

    #[test]
    fn regex_selection_and_invalid_pattern() {
        let mut fixture = HelixApp::new();
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char('%'), KeyModifiers::NONE));
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char('s'), KeyModifiers::NONE));
        type_keys(&mut fixture.app, "o");
        handle_helix_mode(&mut fixture.app, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(fixture.app.editor.helix_selection_set().unwrap().selections.len(), 2);
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char('s'), KeyModifiers::NONE));
        type_keys(&mut fixture.app, "[");
        handle_helix_mode(&mut fixture.app, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(fixture.app.editor.helix_selection_set().unwrap().selections.len(), 2);
        assert!(fixture.app.editor.helix.status_message.as_deref().unwrap_or("").starts_with("Regex:"));
    }

    #[test]
    fn open_lines_insert_in_the_new_line() {
        let mut below = HelixApp::new();
        handle_helix_mode(&mut below.app, key(KeyCode::Char('o'), KeyModifiers::NONE));
        type_keys(&mut below.app, "new");
        handle_helix_mode(&mut below.app, key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(below.app.editor.text(), "hello world\nnew");

        let mut above = HelixApp::new();
        handle_helix_mode(&mut above.app, key(KeyCode::Char('O'), KeyModifiers::SHIFT));
        type_keys(&mut above.app, "new");
        handle_helix_mode(&mut above.app, key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(above.app.editor.text(), "new\nhello world");
    }

    #[test]
    fn line_selection_deletes_line_and_newline() {
        let mut fixture = HelixApp::new();
        fixture.app.editor.replace(crate::editor::Editor::from_text("hello world\nnext"));
        fixture.app.editor.helix_begin();
        fixture.app.editor.helix_set_selections(vec![HelixSelection::caret(0)], 0);
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(fixture.app.editor.helix_selected_texts(), vec!["hello world\n"]);
        handle_helix_mode(&mut fixture.app, key(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(fixture.app.editor.text(), "next");
    }

    #[test]
    fn repeated_x_keeps_extending_the_line_selection() {
        let mut fixture = HelixApp::with_text("one\ntwo\nthree\nfour");
        fixture.keys("xxx");
        assert_eq!(fixture.selected(), vec!["one\ntwo\nthree\n"]);
        fixture.select(vec![HelixSelection::caret(4)], 0);
        fixture.keys("2x");
        assert_eq!(fixture.selected(), vec!["two\nthree\n"]);
    }

    #[test]
    fn word_motion_then_delete_removes_the_word() {
        let mut fixture = HelixApp::new();
        fixture.keys("wd");
        assert_eq!(fixture.text(), "world");
        fixture.keys("ec");
        fixture.keys("earth");
        fixture.press(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(fixture.text(), "earth");
    }

    #[test]
    fn insert_then_escape_keeps_the_cursor_on_the_original_character() {
        let mut fixture = HelixApp::new();
        fixture.keys("iX");
        fixture.press(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(fixture.app.editor.helix_primary(), Some(HelixSelection::caret(1)));
        fixture.keys("aY");
        fixture.press(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(fixture.text(), "XhYello world");
        assert_eq!(fixture.app.editor.helix_primary(), Some(HelixSelection::caret(2)));
        fixture.keys("a");
        fixture.press(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(fixture.app.editor.helix_primary(), Some(HelixSelection::caret(2)));
    }

    #[test]
    fn escape_in_normal_mode_keeps_all_selections() {
        let mut fixture = HelixApp::new();
        fixture.select(vec![HelixSelection::caret(0), HelixSelection::caret(6)], 0);
        fixture.keys("v");
        fixture.press(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(fixture.app.editor.helix.mode, HelixMode::Normal);
        assert_eq!(fixture.app.editor.helix_selection_set().unwrap().selections.len(), 2);
    }

    #[test]
    fn count_is_not_leaked_into_the_next_command() {
        let mut fixture = HelixApp::with_text("a\nb\nc\nd\ne");
        fixture.keys("3");
        fixture.press(KeyCode::Char('s'), KeyModifiers::CONTROL);
        fixture.keys("j");
        assert_eq!(fixture.app.editor.helix_primary(), Some(HelixSelection::caret(2)));
    }

    #[test]
    fn join_keeps_indentation_and_inserts_a_space() {
        let mut fixture = HelixApp::with_text("  - item\n    more\n\nlast");
        fixture.keys("J");
        assert_eq!(fixture.text(), "  - item more\n\nlast");
        fixture.select(vec![HelixSelection::caret(12)], 0);
        fixture.keys("J");
        assert_eq!(fixture.text(), "  - item more\nlast");
        fixture.select(vec![HelixSelection::caret(0)], 0);
        fixture.press(KeyCode::Char('J'), KeyModifiers::ALT);
        assert_eq!(fixture.text(), "  - item more last");
        assert_eq!(fixture.selected(), vec![" "]);
    }

    #[test]
    fn replace_keeps_line_breaks_and_selection() {
        let mut fixture = HelixApp::with_text("ab\ncd");
        fixture.keys("%rx");
        assert_eq!(fixture.text(), "xx\nxx");
        assert_eq!(fixture.selected(), vec!["xx\nxx"]);
    }

    #[test]
    fn trim_selections_only_changes_selections() {
        let mut fixture = HelixApp::with_text("  word  ");
        fixture.keys("%_");
        assert_eq!(fixture.text(), "  word  ");
        assert_eq!(fixture.selected(), vec!["word"]);
    }

    #[test]
    fn align_pads_before_each_selection() {
        let mut fixture = HelixApp::with_text("a = 1\nlong = 2");
        fixture.keys("%s=");
        fixture.press(KeyCode::Enter, KeyModifiers::NONE);
        fixture.keys("&");
        assert_eq!(fixture.text(), "a    = 1\nlong = 2");
        assert_eq!(fixture.selected(), vec!["=", "="]);
    }

    #[test]
    fn space_p_pastes_a_clipboard_image_after_the_selection() {
        let mut fixture = HelixApp::new();
        fixture.clipboard.set_image_png(b"png bytes".to_vec());
        fixture.select(vec![HelixSelection::spanning(0, 5, false)], 0);
        fixture.keys(" p");
        let attachments: Vec<PathBuf> = fs::read_dir(fixture.root.join("vault/attachments")).unwrap().map(|entry| entry.unwrap().path()).collect();
        assert_eq!(attachments.len(), 1);
        let name = attachments[0].file_name().unwrap().to_str().unwrap().replace(' ', "%20");
        assert_eq!(fixture.text(), format!("hello![](attachments/{name}) world"));
    }

    #[test]
    fn insert_mode_ctrl_v_and_ctrl_r_plus_paste_clipboard_images() {
        let mut fixture = HelixApp::new();
        fixture.clipboard.set_image_png(b"png bytes".to_vec());
        fixture.select(vec![HelixSelection::caret(5)], 0);
        fixture.keys("i");
        fixture.press(KeyCode::Char('v'), KeyModifiers::CONTROL);
        fixture.press(KeyCode::Char('r'), KeyModifiers::CONTROL);
        fixture.keys("+");
        let mut attachments: Vec<String> = fs::read_dir(fixture.root.join("vault/attachments")).unwrap().map(|entry| entry.unwrap().file_name().to_str().unwrap().replace(' ', "%20")).collect();
        attachments.sort_by(|left, right| left.len().cmp(&right.len()).then_with(|| left.cmp(right)));
        assert_eq!(attachments.len(), 2);
        assert_eq!(fixture.text(), format!("hello![](attachments/{})![](attachments/{}) world", attachments[0], attachments[1]));
        assert_eq!(fixture.app.editor.helix.mode, HelixMode::Insert);
    }

    #[test]
    fn linewise_paste_goes_below_the_current_line() {
        let mut fixture = HelixApp::with_text("one\ntwo");
        fixture.keys("xyp");
        assert_eq!(fixture.text(), "one\none\ntwo");
        fixture.select(vec![HelixSelection::caret(8)], 0);
        fixture.keys("p");
        assert_eq!(fixture.text(), "one\none\ntwo\none");
        assert_eq!(fixture.selected(), vec!["one"]);
    }

    #[test]
    fn find_selects_up_to_the_character_and_till_repeats() {
        let mut fixture = HelixApp::with_text("a,b,c,d");
        fixture.keys("f,");
        assert_eq!(fixture.selected(), vec!["a,"]);
        fixture.select(vec![HelixSelection::caret(0)], 0);
        fixture.keys("t,");
        assert_eq!(fixture.selected(), vec!["a,b"]);
        fixture.press(KeyCode::Char('.'), KeyModifiers::ALT);
        assert_eq!(fixture.selected(), vec!["b,c"]);
    }

    #[test]
    fn regex_prompts_use_multi_line_and_smart_case() {
        let mut fixture = HelixApp::with_text("Alpha\nbeta\nalpha");
        fixture.keys("%s^");
        fixture.press(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(fixture.app.editor.helix_selection_set().unwrap().selections.len(), 1);
        fixture.keys("%s^.");
        fixture.press(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(fixture.selected(), vec!["A", "b", "a"]);
        fixture.keys("%salpha");
        fixture.press(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(fixture.selected(), vec!["Alpha", "alpha"]);
        fixture.keys("%sAlpha");
        fixture.press(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(fixture.selected(), vec!["Alpha"]);
    }

    #[test]
    fn search_moves_past_the_current_match() {
        let mut fixture = HelixApp::with_text("foo bar foo bar foo");
        fixture.keys("/foo");
        fixture.press(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(fixture.app.editor.helix_primary(), Some(HelixSelection { anchor: 8, head: 10 }));
        fixture.keys("n");
        assert_eq!(fixture.app.editor.helix_primary(), Some(HelixSelection { anchor: 16, head: 18 }));
        fixture.keys("vn");
        assert_eq!(fixture.app.editor.helix_selection_set().unwrap().selections.len(), 2);
        assert_eq!(fixture.app.editor.helix.status_message.as_deref(), Some("Wrapped around document"));
    }

    #[test]
    fn macro_records_capital_q_typed_in_insert_mode() {
        let mut fixture = HelixApp::with_text("x\nx");
        fixture.keys("Q");
        fixture.keys("IQ");
        fixture.press(KeyCode::Esc, KeyModifiers::NONE);
        fixture.keys("jQ");
        fixture.keys("q");
        assert_eq!(fixture.text(), "Qx\nQx");
    }

    #[test]
    fn ctrl_r_in_insert_mode_does_not_leak_the_register() {
        let mut fixture = HelixApp::new();
        fixture.keys("\"ayi");
        fixture.press(KeyCode::Char('r'), KeyModifiers::CONTROL);
        fixture.keys("a");
        fixture.press(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(fixture.text(), "hhello world");
        assert_eq!(fixture.app.editor.helix.selected_register, None);
    }

    #[test]
    fn backward_deletes_with_mixed_cursors_keep_offsets() {
        let mut fixture = HelixApp::with_text("ab\ncd\nef");
        fixture.select(vec![HelixSelection::caret(1), HelixSelection::caret(3), HelixSelection::caret(7)], 0);
        fixture.keys("i");
        fixture.press(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(fixture.text(), "bcd\nf");
        let carets: Vec<_> = fixture.app.editor.helix_selection_set().unwrap().selections.iter().map(|selection| selection.head).collect();
        assert_eq!(carets, vec![0, 1, 4]);
    }

    #[test]
    fn numbers_under_the_cursor_are_incremented_whole() {
        let mut fixture = HelixApp::with_text("x 19 y");
        fixture.select(vec![HelixSelection::caret(3)], 0);
        fixture.press(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(fixture.text(), "x 20 y");
        fixture.keys("5");
        fixture.press(KeyCode::Char('x'), KeyModifiers::CONTROL);
        assert_eq!(fixture.text(), "x 15 y");
    }

    #[test]
    fn indent_and_unindent_keep_carets_on_their_text() {
        let mut fixture = HelixApp::with_text("a\n\nb");
        fixture.keys("%>");
        assert_eq!(fixture.text(), "\ta\n\n\tb");
        fixture.select(vec![HelixSelection::caret(1)], 0);
        fixture.keys("<");
        assert_eq!(fixture.text(), "a\n\n\tb");
        assert_eq!(fixture.app.editor.helix_primary(), Some(HelixSelection::caret(0)));
    }

    #[test]
    fn copy_selection_skips_short_lines_and_moves_primary() {
        let mut fixture = HelixApp::with_text("abcd\na\nabcd");
        fixture.select(vec![HelixSelection::caret(3)], 0);
        fixture.keys("C");
        let set = fixture.app.editor.helix_selection_set().unwrap().clone();
        assert_eq!(set.selections, vec![HelixSelection::caret(3), HelixSelection::caret(10)]);
        assert_eq!(set.primary, 1);
    }

    #[test]
    fn follow_link_requires_a_saved_buffer_for_notes() {
        let mut fixture = HelixApp::with_text("see [[other]]");
        fixture.keys("iX");
        fixture.press(KeyCode::Esc, KeyModifiers::NONE);
        fixture.select(vec![HelixSelection::caret(8)], 0);
        fixture.keys("gf");
        assert_eq!(fixture.app.editor.mode, Mode::Edit);
        assert_eq!(fixture.app.editor.helix.status_message.as_deref(), Some("Save with :w before following a link"));
        assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "see [[other]]");
    }

    #[test]
    fn context_menu_takes_keyboard_input_before_helix() {
        let mut fixture = HelixApp::new();
        fixture.app.editor.context_menu_state = ContextMenuState::Open { x: 0, y: 0, selected_index: 0 };
        handle_edit_mode(&mut fixture.app, key(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(fixture.text(), "hello world");
        handle_edit_mode(&mut fixture.app, key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(fixture.app.editor.context_menu_state, ContextMenuState::None);
    }

    #[test]
    fn bracketed_paste_goes_into_an_open_prompt() {
        let mut fixture = HelixApp::new();
        fixture.keys("/");
        handle_paste_event(&mut fixture.app, "world\nignored".to_string());
        assert_eq!(fixture.app.editor.helix.prompt, "world");
        assert_eq!(fixture.text(), "hello world");
    }

    #[test]
    fn view_scroll_keeps_the_cursor_visible() {
        let text = (0..30).map(|row| format!("line {row}")).collect::<Vec<_>>().join("\n");
        let mut fixture = HelixApp::with_text(&text);
        fixture.app.update_editor_scroll(5);
        for _ in 0..4 {
            fixture.keys("zj");
        }
        let row = fixture.app.editor.helix_position(fixture.app.editor.helix_primary().unwrap().head).row;
        assert!(row >= fixture.app.editor.editor_scroll_top, "cursor row {row} is above the viewport");
    }

    #[test]
    fn pending_label_is_readable() {
        let mut fixture = HelixApp::new();
        fixture.keys("2m");
        assert_eq!(fixture.app.editor.helix.pending_label(), "m");
        fixture.keys("r(");
        assert_eq!(fixture.app.editor.helix.pending_label(), "mr(");
    }
}
