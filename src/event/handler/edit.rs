use super::*;

pub(super) fn handle_edit_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    match app.state.keymap.resolve(key, |command| command == AppCommand::ToggleEditorMode) {
        KeyResolution::Command(AppCommand::ToggleEditorMode) => {
            switch_editing_mode(app);
            return;
        }
        KeyResolution::Pending => return,
        KeyResolution::Command(_) | KeyResolution::NoMatch => {}
    }
    if key.code == KeyCode::F(1) {
        app.state.dialog = DialogState::Help;
        return;
    }
    if handle_wiki_autocomplete(app, key) {
        app.request_highlight_update();
        return;
    }
    if let ContextMenuState::Open { x, y, selected_index } = app.editor.context_menu_state {
        let items = ContextMenuItem::all();
        match key.code {
            KeyCode::Esc => {
                app.editor.context_menu_state = ContextMenuState::None;
            }
            KeyCode::Enter => {
                if let Some(&action) = items.get(selected_index) {
                    execute_context_menu_action(app, action);
                }
                app.editor.context_menu_state = ContextMenuState::None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let new_index = (selected_index + 1) % items.len();
                app.editor.context_menu_state = ContextMenuState::Open { x, y, selected_index: new_index };
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let new_index = if selected_index == 0 { items.len() - 1 } else { selected_index - 1 };
                app.editor.context_menu_state = ContextMenuState::Open { x, y, selected_index: new_index };
            }
            _ => {}
        }
        return;
    }
    if let Some(delete_type) = app.editor.pending_delete {
        match key.code {
            KeyCode::Char('d') => {
                app.editor.pending_delete = None;
                app.editor.cut();
                if delete_type == DeleteType::Line {
                    app.editor.delete_newline();
                }
            }
            KeyCode::Esc => {
                app.editor.pending_delete = None;
                app.editor.cancel_selection();
            }
            _ => {
                app.editor.pending_delete = None;
                app.editor.cancel_selection();
                dispatch_editor_input(app, key);
            }
        }
        app.request_highlight_update();
        app.update_editor_block();
        return;
    }
    if handle_editor_command(app, key) {
        return;
    }
    dispatch_editor_input(app, key);
    app.request_highlight_update();
    app.update_editor_block();
}

fn handle_editor_command(app: &mut App, key: crossterm::event::KeyEvent) -> bool {
    let accepts_line_commands = app.state.config.editor.mode == EditingMode::Standard || matches!(app.editor.vim.mode.input_mode(), VimInputMode::Normal | VimInputMode::Insert | VimInputMode::Replace);
    let cursor_row = app.editor.cursor().0;
    let can_toggle_fold = accepts_line_commands && !app.editor.has_selection() && app.editor.foldable_heading_level(cursor_row).is_some();
    let resolution = app.state.keymap.resolve_editor(key, |command| match command {
        AppCommand::InsertTask => accepts_line_commands,
        AppCommand::ToggleEditorFold => can_toggle_fold,
        _ => false,
    });
    match resolution {
        KeyResolution::Command(AppCommand::InsertTask) => {
            if app.editor.insert_task_on_current_line() {
                app.request_highlight_update();
            }
            true
        }
        KeyResolution::Command(AppCommand::ToggleEditorFold) => {
            if app.editor.toggle_current_heading_fold() {
                app.state.needs_full_clear = true;
                app.editor.editor_scroll_top = app.editor.scroll_offset();
                app.request_highlight_update();
            }
            true
        }
        KeyResolution::Pending => true,
        KeyResolution::Command(_) | KeyResolution::NoMatch => false,
    }
}

fn dispatch_editor_input(app: &mut App, key: crossterm::event::KeyEvent) {
    if app.state.config.editor.mode == EditingMode::Standard {
        handle_standard_mode(app, key);
        return;
    }
    match app.editor.vim.mode.input_mode() {
        VimInputMode::Normal => handle_vim_normal_mode(app, key),
        VimInputMode::Insert => handle_vim_insert_mode(app, key),
        VimInputMode::Replace => handle_vim_replace_mode(app, key),
        VimInputMode::Visual => handle_vim_visual_mode(app, key),
        VimInputMode::Command => handle_vim_command_mode(app, key),
        VimInputMode::Search => handle_vim_search_mode(app, key),
        VimInputMode::SearchLocked => handle_vim_search_locked_mode(app, key),
    }
}
