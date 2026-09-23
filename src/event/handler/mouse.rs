use super::*;

pub(super) fn handle_mouse_event(app: &mut App, mouse: crossterm::event::MouseEvent) {
    app.state.keymap.reset_pending();
    if app.state.dialog == DialogState::EditorModeSelector {
        return;
    }
    let mouse_x = mouse.column;
    let mouse_y = mouse.row;
    if let ContextMenuState::Open { x, y, selected_index: _ } = app.editor.context_menu_state {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(action) = get_context_menu_click(mouse_x, mouse_y, x, y) {
                    execute_context_menu_action(app, action);
                }
                app.editor.context_menu_state = ContextMenuState::None;
                return;
            }
            MouseEventKind::Moved => {
                if let Some(new_idx) = get_context_menu_hover_index(mouse_x, mouse_y, x, y) {
                    app.editor.context_menu_state = ContextMenuState::Open { x, y, selected_index: new_idx };
                }
                return;
            }
            _ => {
                if matches!(mouse.kind, MouseEventKind::Down(_)) {
                    app.editor.context_menu_state = ContextMenuState::None;
                }
                return;
            }
        }
    }
    if !matches!(app.search.search_picker, SearchPickerState::Closed) {
        if app.is_inside_search_picker(mouse_x, mouse_y) {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    app.search_picker_scroll_up();
                    return;
                }
                MouseEventKind::ScrollDown => {
                    app.search_picker_scroll_down();
                    return;
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    match app.search_picker_click(mouse_x, mouse_y) {
                        2 => {
                            app.select_search_picker_result();
                        }
                        1 => {}
                        _ => {}
                    }
                    return;
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    return;
                }
                _ => {}
            }
        } else if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            app.close_search_picker();
            return;
        }
        return;
    }
    if app.state.dialog == DialogState::None && !app.state.show_welcome && app.state.show_changelog {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let pointer = ratatui::layout::Position::new(mouse_x, mouse_y);
                if let Some(url) = app.state.changelog_links.iter().find(|(area, _)| area.contains(pointer)).map(|(_, url)| url.clone()) {
                    app.open_link(&url);
                }
            }
            MouseEventKind::ScrollDown => {
                app.state.changelog_scroll = app.state.changelog_scroll.saturating_add(1);
            }
            MouseEventKind::ScrollUp => {
                app.state.changelog_scroll = app.state.changelog_scroll.saturating_sub(1);
            }
            _ => {}
        }
        return;
    }
    if app.state.dialog == DialogState::GraphView {
        handle_graph_view_mouse(app, mouse);
        return;
    }
    if app.state.dialog == DialogState::TaskView {
        handle_task_view_mouse(app, mouse);
        return;
    }
    if app.editor.mode == Mode::Edit {
        handle_edit_mode_mouse(app, mouse);
        return;
    }
    if app.editor.mode == Mode::Normal && app.state.dialog == DialogState::None && !app.state.show_welcome && !app.state.show_changelog {
        let in_content_area = mouse_x >= app.state.content_area.x && mouse_x < app.state.content_area.x + app.state.content_area.width && mouse_y >= app.state.content_area.y && mouse_y < app.state.content_area.y + app.state.content_area.height;
        if !in_content_area && app.canvas_editor_active() && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) && !app.canvas_commit_node_edit() {
            return;
        }
        if !in_content_area && app.active_document_kind() == Some(crate::vault::VaultFileKind::Canvas) && matches!(mouse.kind, MouseEventKind::Moved) {
            app.structured.canvas.hovered_node = None;
            app.structured.canvas.hovered_edge = None;
            app.structured.canvas.hovered_resize = None;
            app.structured.canvas.shortcut_toggle_hovered = false;
        }
        if (in_content_area || app.canvas_interaction_active() || app.canvas_overlay_active()) && handle_structured_document_mouse(app, mouse) {
            return;
        }
        match mouse.kind {
            MouseEventKind::Moved => {
                if in_content_area {
                    let hovered_inline_image = app.state.inline_image_rects.iter().find(|image| mouse_x >= image.rect.x && mouse_x < image.rect.x + image.rect.width && mouse_y >= image.rect.y && mouse_y < image.rect.y + image.rect.height);
                    app.state.mouse_hover_inline_image = hovered_inline_image.map(|image| (image.item_index, image.selection_index));
                    let hovered_item = app.state.content_item_rects.iter().find(|(_, rect)| mouse_y >= rect.y && mouse_y < rect.y + rect.height).map(|(idx, _)| *idx);
                    if let Some(idx) = hovered_item {
                        if app.state.mouse_hover_inline_image.is_some() || app.item_has_link_at(idx) || app.item_is_image_at(idx).is_some() {
                            app.state.mouse_hover_item = Some(idx);
                        } else {
                            app.state.mouse_hover_item = None;
                        }
                    } else {
                        app.state.mouse_hover_item = None;
                    }
                } else {
                    app.state.mouse_hover_item = None;
                    app.state.mouse_hover_inline_image = None;
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let in_sidebar_area = app.state.sidebar_area.width > 0 && mouse_x >= app.state.sidebar_area.x && mouse_x < app.state.sidebar_area.x + app.state.sidebar_area.width && mouse_y >= app.state.sidebar_area.y && mouse_y < app.state.sidebar_area.y + app.state.sidebar_area.height;
                let in_outline_area = app.state.outline_area.width > 0 && mouse_x >= app.state.outline_area.x && mouse_x < app.state.outline_area.x + app.state.outline_area.width && mouse_y >= app.state.outline_area.y && mouse_y < app.state.outline_area.y + app.state.outline_area.height;
                if in_sidebar_area {
                    let inner_y = mouse_y.saturating_sub(app.state.sidebar_area.y + 1); // +1 for top border
                    let clicked_index = inner_y as usize;
                    if clicked_index < app.vault.sidebar_items.len() {
                        app.vault.selected_sidebar_index = clicked_index;
                        app.state.focus = Focus::Sidebar;
                        execute_app_command(app, AppCommand::Activate);
                    }
                } else if in_outline_area {
                    let inner_y = mouse_y.saturating_sub(app.state.outline_area.y + 1); // +1 for top border
                    let clicked_index = inner_y as usize;
                    if clicked_index < app.document.outline.len() {
                        app.document.outline_state.select(Some(clicked_index));
                        app.state.focus = Focus::Outline;
                        execute_app_command(app, AppCommand::Activate);
                    }
                } else if in_content_area {
                    let clicked_inline_image = app.state.inline_image_rects.iter().find(|image| mouse_x >= image.rect.x && mouse_x < image.rect.x + image.rect.width && mouse_y >= image.rect.y && mouse_y < image.rect.y + image.rect.height).cloned();
                    if let Some(image) = clicked_inline_image {
                        app.state.focus = Focus::Content;
                        app.document.content_cursor = image.item_index;
                        app.document.selected_link_index = image.selection_index;
                        open_selected_content_target(app);
                        return;
                    }
                    let clicked_item = app.state.content_item_rects.iter().find(|(_, rect)| mouse_y >= rect.y && mouse_y < rect.y + rect.height).copied();
                    if let Some((idx, item_rect)) = clicked_item {
                        if app.is_content_item_visible(idx) {
                            app.document.content_cursor = idx;
                            app.document.selected_link_index = 0;
                        }
                        let clicked_rendered_col = crate::ui::content_item_click_col(app, idx, item_rect, mouse_x, mouse_y);
                        if mouse_y == item_rect.y && app.is_click_on_task_checkbox(idx, mouse_x, app.state.content_area.x) {
                            app.toggle_task_at(idx);
                        } else if let Some(url) = clicked_rendered_col.and_then(|col| app.find_clicked_link_at_col(idx, col)) {
                            app.open_link(&url);
                        } else if let Some(wiki_link) = clicked_rendered_col.and_then(|col| app.find_clicked_wiki_link_at_col(idx, col)) {
                            if wiki_link.is_valid {
                                app.navigate_to_wiki_link_with_heading(&wiki_link.target, wiki_link.heading.as_deref());
                            } else {
                                app.editor.pending_wiki_target = Some(wiki_link.target);
                                app.state.dialog = DialogState::CreateWikiNote;
                            }
                        } else if let Some(path) = app.item_is_image_at(idx) {
                            app.open_path_or_url(path);
                        } else if app.item_is_details_at(idx) {
                            app.toggle_details_at(idx);
                        } else if app.is_heading_at(idx) {
                            app.toggle_heading_fold_at(idx);
                        }
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                execute_app_command(app, AppCommand::MoveDown);
            }
            MouseEventKind::ScrollUp => {
                execute_app_command(app, AppCommand::MoveUp);
            }
            _ => {}
        }
    }
}

fn handle_structured_document_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) -> bool {
    match app.active_document_kind() {
        Some(crate::vault::VaultFileKind::Base) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                if app.structured.base.column_left_rect.is_some_and(|rect| rect.contains(pointer)) {
                    app.base_move_column(-1);
                    app.state.focus = Focus::Content;
                } else if app.structured.base.column_right_rect.is_some_and(|rect| rect.contains(pointer)) {
                    app.base_move_column(1);
                    app.state.focus = Focus::Content;
                } else if let Some((row, _)) = app.structured.base.row_rects.iter().find(|(_, rect)| rect.contains(pointer)).copied() {
                    app.structured.base.selected_row = row;
                    app.state.focus = Focus::Content;
                }
                true
            }
            MouseEventKind::ScrollDown => {
                app.base_move_selection(1);
                true
            }
            MouseEventKind::ScrollUp => {
                app.base_move_selection(-1);
                true
            }
            _ => false,
        },
        Some(crate::vault::VaultFileKind::Canvas) => {
            if app.canvas_overlay_active() && handle_canvas_overlay_mouse(app, mouse) {
                return true;
            }
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                    if app.structured.canvas.shortcut_toggle_rect.is_some_and(|rect| rect.contains(pointer)) {
                        app.canvas_toggle_shortcuts();
                        return true;
                    }
                    if matches!(app.structured.canvas.interaction, crate::app::CanvasInteraction::Connecting { .. }) {
                        if let Some((node, rect)) = app.structured.canvas.node_rects.iter().rev().find(|(_, rect)| rect.contains(pointer)).copied() {
                            app.canvas_end_pointer_interaction(Some((node, Some(canvas_side_at(rect, pointer)))));
                            return true;
                        }
                    }
                    if let Some((side, _)) = app.structured.canvas.handle_rects.iter().find(|(_, rect)| rect.contains(pointer)).copied() {
                        app.canvas_begin_connect(Some(side), Some((mouse.column, mouse.row)));
                        return true;
                    }
                    if let Some((handle, _)) = app.structured.canvas.resize_rects.iter().find(|(_, rect)| rect.contains(pointer)).copied() {
                        app.canvas_begin_node_resize(app.structured.canvas.selected_node, handle, (mouse.column, mouse.row));
                        return true;
                    }
                    if let Some(editor_node) = app.structured.canvas.editor.as_ref().map(|editor| editor.node) {
                        if app.canvas_editor_contains(pointer) {
                            app.canvas_edit_place_cursor(pointer);
                            return true;
                        }
                        let inside_node = app.structured.canvas.node_rects.iter().any(|(node, rect)| *node == editor_node && rect.contains(pointer));
                        if inside_node || !app.canvas_commit_node_edit() {
                            return true;
                        }
                    }
                    if let Some((node, _)) = app.structured.canvas.node_rects.iter().rev().find(|(_, rect)| rect.contains(pointer)).copied() {
                        app.structured.canvas.last_background_click = None;
                        let double_click = app.structured.canvas.last_click.is_some_and(|(when, previous)| previous == node && when.elapsed() < std::time::Duration::from_millis(400));
                        app.structured.canvas.last_click = Some((std::time::Instant::now(), node));
                        app.canvas_begin_node_drag(node, (mouse.column, mouse.row));
                        if double_click {
                            app.structured.canvas.last_click = None;
                            app.structured.canvas.interaction = crate::app::CanvasInteraction::Idle;
                            app.canvas_activate_selected_node();
                        }
                        return true;
                    }
                    if let Some((edge, _)) = app.structured.canvas.edge_cells.iter().rev().find(|(_, position)| *position == pointer).copied() {
                        app.structured.canvas.last_click = None;
                        app.structured.canvas.last_background_click = None;
                        app.canvas_select_edge(edge);
                        return true;
                    }
                    if app.structured.canvas.view_area.contains(pointer) {
                        app.structured.canvas.last_click = None;
                        let double_click = app.structured.canvas.last_background_click.is_some_and(|(when, previous)| when.elapsed() < std::time::Duration::from_millis(400) && previous.x.abs_diff(pointer.x) <= 1 && previous.y.abs_diff(pointer.y) <= 1);
                        app.structured.canvas.last_background_click = Some((std::time::Instant::now(), pointer));
                        if double_click {
                            app.structured.canvas.last_background_click = None;
                            app.canvas_open_context_menu(pointer, crate::app::CanvasMenuTarget::Background);
                            app.canvas_execute_menu_action(crate::app::CanvasMenuAction::AddText);
                            return true;
                        }
                        app.canvas_begin_pan((mouse.column, mouse.row));
                    }
                    true
                }
                MouseEventKind::Down(MouseButton::Middle) => {
                    app.structured.canvas.last_click = None;
                    app.structured.canvas.last_background_click = None;
                    let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                    if app.structured.canvas.view_area.contains(pointer) {
                        app.canvas_begin_pan((mouse.column, mouse.row));
                    }
                    true
                }
                MouseEventKind::Down(MouseButton::Right) => {
                    app.structured.canvas.last_click = None;
                    app.structured.canvas.last_background_click = None;
                    let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                    if !app.structured.canvas.view_area.contains(pointer) {
                        return true;
                    }
                    if app.canvas_editor_active() && !app.canvas_commit_node_edit() {
                        return true;
                    }
                    if let Some((node, _)) = app.structured.canvas.node_rects.iter().rev().find(|(_, rect)| rect.contains(pointer)).copied() {
                        app.canvas_open_context_menu(pointer, crate::app::CanvasMenuTarget::Node(node));
                    } else if let Some((edge, _)) = app.structured.canvas.edge_cells.iter().rev().find(|(_, position)| *position == pointer).copied() {
                        app.canvas_open_context_menu(pointer, crate::app::CanvasMenuTarget::Edge(edge));
                    } else {
                        app.canvas_open_context_menu(pointer, crate::app::CanvasMenuTarget::Background);
                    }
                    true
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                    app.structured.canvas.hovered_node = app.structured.canvas.node_rects.iter().rev().find(|(_, rect)| rect.contains(pointer)).map(|(node, _)| *node);
                    app.canvas_pointer_drag_with_aspect((mouse.column, mouse.row), mouse.modifiers.contains(KeyModifiers::SHIFT));
                    true
                }
                MouseEventKind::Drag(MouseButton::Middle) => {
                    app.canvas_pointer_drag((mouse.column, mouse.row));
                    true
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                    let target = app.structured.canvas.node_rects.iter().rev().find(|(_, rect)| rect.contains(pointer)).map(|(node, rect)| (*node, Some(canvas_side_at(*rect, pointer))));
                    app.canvas_end_pointer_interaction(target);
                    true
                }
                MouseEventKind::Up(MouseButton::Middle) => {
                    app.canvas_end_pointer_interaction(None);
                    true
                }
                MouseEventKind::Moved => {
                    let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                    app.structured.canvas.shortcut_toggle_hovered = app.structured.canvas.shortcut_toggle_rect.is_some_and(|rect| rect.contains(pointer));
                    app.structured.canvas.hovered_node = app.structured.canvas.node_rects.iter().rev().find(|(_, rect)| rect.contains(pointer)).map(|(node, _)| *node);
                    app.structured.canvas.hovered_edge = if app.structured.canvas.hovered_node.is_none() { app.structured.canvas.edge_cells.iter().rev().find(|(_, position)| *position == pointer).map(|(edge, _)| *edge) } else { None };
                    app.structured.canvas.hovered_resize = app.structured.canvas.resize_rects.iter().find(|(_, rect)| rect.contains(pointer)).map(|(handle, _)| (*handle, pointer));
                    true
                }
                MouseEventKind::ScrollUp => {
                    let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                    if app.canvas_editor_contains(pointer) {
                        app.canvas_edit_scroll(-3);
                    } else {
                        app.canvas_zoom_at(1.1, Some((mouse.column, mouse.row)));
                    }
                    true
                }
                MouseEventKind::ScrollDown => {
                    let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                    if app.canvas_editor_contains(pointer) {
                        app.canvas_edit_scroll(3);
                    } else {
                        app.canvas_zoom_at(1.0 / 1.1, Some((mouse.column, mouse.row)));
                    }
                    true
                }
                _ => false,
            }
        }
        Some(crate::vault::VaultFileKind::Markdown) | None => false,
    }
}

fn handle_canvas_overlay_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) -> bool {
    let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Right | MouseButton::Middle) => {
            app.canvas_close_overlay();
            false
        }
        MouseEventKind::Down(MouseButton::Left) => match &app.structured.canvas.overlay {
            crate::app::CanvasOverlay::Menu(menu) => {
                let action = menu.item_rects.iter().find(|(_, rect)| rect.contains(pointer)).map(|(action, _)| *action);
                if let Some(action) = action {
                    app.canvas_execute_menu_action(action);
                } else {
                    app.canvas_close_overlay();
                }
                true
            }
            crate::app::CanvasOverlay::FilePicker(picker) => {
                let clicked = picker.result_rects.iter().find(|(_, rect)| rect.contains(pointer)).map(|(index, _)| *index);
                if let Some(index) = clicked {
                    let now = std::time::Instant::now();
                    let double_click = picker.last_click.is_some_and(|(when, previous)| previous == index && now.duration_since(when) < std::time::Duration::from_millis(400));
                    if let crate::app::CanvasOverlay::FilePicker(picker) = &mut app.structured.canvas.overlay {
                        picker.selected_index = index;
                        picker.last_click = Some((now, index));
                    }
                    if double_click {
                        app.canvas_activate_file_picker_selection();
                    }
                } else if !picker.area.contains(pointer) {
                    app.canvas_close_overlay();
                }
                true
            }
            crate::app::CanvasOverlay::None => false,
        },
        MouseEventKind::Moved => {
            match &app.structured.canvas.overlay {
                crate::app::CanvasOverlay::Menu(menu) => {
                    let hovered = menu.item_rects.iter().position(|(_, rect)| rect.contains(pointer));
                    if let (Some(index), crate::app::CanvasOverlay::Menu(menu)) = (hovered, &mut app.structured.canvas.overlay) {
                        menu.selected_index = index;
                    }
                }
                crate::app::CanvasOverlay::FilePicker(picker) => {
                    let hovered = picker.result_rects.iter().find(|(_, rect)| rect.contains(pointer)).map(|(index, _)| *index);
                    if let (Some(index), crate::app::CanvasOverlay::FilePicker(picker)) = (hovered, &mut app.structured.canvas.overlay) {
                        picker.selected_index = index;
                    }
                }
                crate::app::CanvasOverlay::None => return false,
            }
            true
        }
        MouseEventKind::ScrollUp => {
            if matches!(app.structured.canvas.overlay, crate::app::CanvasOverlay::Menu(_)) {
                app.canvas_menu_move_selection(-1);
            } else {
                app.canvas_file_picker_move_selection(-1);
            }
            true
        }
        MouseEventKind::ScrollDown => {
            if matches!(app.structured.canvas.overlay, crate::app::CanvasOverlay::Menu(_)) {
                app.canvas_menu_move_selection(1);
            } else {
                app.canvas_file_picker_move_selection(1);
            }
            true
        }
        _ => true,
    }
}

fn canvas_side_at(rect: ratatui::layout::Rect, pointer: ratatui::layout::Position) -> crate::canvas::CanvasSide {
    let distances = [
        (pointer.y.saturating_sub(rect.y), crate::canvas::CanvasSide::Top),
        (rect.right().saturating_sub(1).saturating_sub(pointer.x), crate::canvas::CanvasSide::Right),
        (rect.bottom().saturating_sub(1).saturating_sub(pointer.y), crate::canvas::CanvasSide::Bottom),
        (pointer.x.saturating_sub(rect.x), crate::canvas::CanvasSide::Left),
    ];
    distances.into_iter().min_by_key(|(distance, _)| *distance).map(|(_, side)| side).unwrap_or(crate::canvas::CanvasSide::Right)
}

pub(super) fn handle_paste_event(app: &mut App, text: String) {
    app.state.keymap.reset_pending();
    if app.canvas_editor_active() {
        app.canvas_edit_insert(&text);
        return;
    }
    if app.editor.mode != Mode::Edit {
        return;
    }
    if app.state.config.editor.mode == EditingMode::Helix {
        app.editor.helix.pending = None;
        if app.editor.helix.mode.is_prompt() {
            app.editor.helix.prompt.push_str(text.lines().next().unwrap_or_default());
            app.update_editor_block();
            return;
        }
    }
    paste_into_editor(app, Some(text));
}

pub(super) fn paste_into_editor(app: &mut App, fallback: Option<String>) {
    app.editor.context_menu_state = ContextMenuState::None;
    app.editor.wiki_autocomplete = WikiAutocompleteState::None;
    if app.state.config.editor.mode == EditingMode::Vim && matches!(app.editor.vim.mode, VimMode::Normal | VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock) {
        app.editor.cancel_selection();
        app.editor.vim.mode = VimMode::Insert;
        update_cursor_style(app);
    }
    let paste_text = match clipboard::get_content_as_markdown_from(app.clipboard()) {
        Ok(ClipboardContent::Markdown(md)) => Some(md),
        Ok(ClipboardContent::PlainText(txt)) => Some(txt),
        Ok(ClipboardContent::Empty) => fallback,
        Err(e) => {
            app.show_error_toast(format!("Clipboard: {}", e));
            fallback
        }
    };
    if let Some(paste_text) = paste_text.filter(|text| !text.is_empty()) {
        if paste_text.contains('\n') {
            app.state.needs_full_clear = true;
        }
        if app.state.config.editor.mode == EditingMode::Helix {
            helix_insert_pasted_text(app, paste_text);
        } else {
            app.editor.insert_str(&paste_text);
        }
    } else if app.state.config.editor.mode == EditingMode::Helix {
        if let Ok(Some(text)) = app.clipboard().get_text() {
            helix_insert_pasted_text(app, text);
        }
    } else {
        app.editor.paste();
    }
    app.update_editor_highlights();
    app.update_editor_block();
    if let Some(view_height) = app.editor.editor_view_height.checked_sub(2) {
        if view_height > 0 {
            app.update_editor_scroll(view_height);
        }
    }
}

pub(super) fn handle_edit_mode_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) {
    let mouse_x = mouse.column;
    let mouse_y = mouse.row;
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            app.editor.context_menu_state = ContextMenuState::None;
            if let Some((row, col)) = app.screen_to_editor_coords(mouse_x, mouse_y) {
                let line_count = app.editor.line_count();
                let row = row.min(line_count.saturating_sub(1));
                let line_len = app.editor.line(row).map(|line| line.chars().count()).unwrap_or(0);
                let col = col.min(line_len);
                if app.editor.has_selection() {
                    app.editor.cancel_selection();
                }
                if app.state.config.editor.mode == EditingMode::Vim && app.editor.vim.mode.is_visual() {
                    app.editor.vim.mode = VimMode::Normal;
                    update_cursor_style(app);
                }
                move_editor_cursor_to(app, row, col);
                if app.state.config.editor.mode == EditingMode::Helix {
                    helix_reset_input(app);
                    let offset = app.editor.helix_offset(Position::new(row, col));
                    app.editor.helix_set_selections(vec![crate::editor::HelixSelection::caret(offset)], 0);
                }
                app.editor.mouse_button_held = true;
                app.editor.mouse_drag_start = Some((row as u16, col as u16));
                app.editor.last_mouse_y = mouse_y; // Initialize to prevent stale auto-scroll
                app.update_editor_block();
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            app.editor.context_menu_state = ContextMenuState::Open { x: mouse_x, y: mouse_y, selected_index: 0 };
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.editor.mouse_button_held = false;
            app.editor.mouse_drag_start = None;
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if app.editor.mouse_button_held {
                app.editor.last_mouse_y = mouse_y;
                if app.state.config.editor.mode == EditingMode::Helix {
                    if let Some((row, col)) = app.screen_to_editor_coords(mouse_x, mouse_y) {
                        let (anchor_row, anchor_col) = app.editor.mouse_drag_start.unwrap_or((row as u16, col as u16));
                        let anchor = app.editor.helix_offset(Position::new(anchor_row as usize, anchor_col as usize));
                        let head = app.editor.helix_offset(Position::new(row, col));
                        app.editor.helix_set_selections(vec![crate::editor::HelixSelection { anchor, head }], 0);
                        handle_auto_scroll(app, mouse_y);
                    }
                    return;
                }
                let can_start_selection = app.state.config.editor.mode == EditingMode::Standard || app.editor.vim.mode == VimMode::Normal;
                if !app.editor.has_selection() && can_start_selection {
                    if app.state.config.editor.mode == EditingMode::Vim {
                        app.editor.vim.mode = VimMode::Visual;
                        update_cursor_style(app);
                        app.editor.set_inclusive_selection(true);
                    } else {
                        app.editor.set_inclusive_selection(false);
                    }
                    app.editor.start_selection();
                    app.update_editor_block();
                }
                if app.editor.has_selection() {
                    handle_auto_scroll(app, mouse_y);
                }
                if let Some((row, col)) = app.screen_to_editor_coords(mouse_x, mouse_y) {
                    let line_count = app.editor.line_count();
                    let row = row.min(line_count.saturating_sub(1));
                    let line_len = app.editor.line(row).map(|line| line.chars().count()).unwrap_or(0);
                    let col = col.min(line_len);
                    move_editor_cursor_to(app, row, col);
                }
            }
        }
        MouseEventKind::ScrollUp => {
            let new_top = app.editor.visible_row_at_offset(app.editor.editor_scroll_top, -3);
            if new_top != app.editor.editor_scroll_top {
                app.editor.editor_scroll_top = new_top;
                app.editor.sync_scroll_offset();
            }
            constrain_cursor_to_viewport(app);
        }
        MouseEventKind::ScrollDown => {
            let new_top = app.editor.visible_row_at_offset(app.editor.editor_scroll_top, 3);
            if new_top != app.editor.editor_scroll_top {
                app.editor.editor_scroll_top = new_top;
                app.editor.sync_scroll_offset();
            }
            constrain_cursor_to_viewport(app);
        }
        _ => {}
    }
}

pub(super) fn handle_auto_scroll(app: &mut App, mouse_y: u16) {
    let direction = app.get_auto_scroll_direction(mouse_y);
    if direction == 0 {
        return;
    }
    perform_auto_scroll(app, direction);
}

/// Continuous auto-scroll when mouse is held near edges (called from main loop)
pub(super) fn handle_continuous_auto_scroll(app: &mut App) {
    let direction = app.get_auto_scroll_direction(app.editor.last_mouse_y);
    if direction == 0 {
        return;
    }
    perform_auto_scroll(app, direction);
}

/// Perform the actual scrolling in the given direction
pub(super) fn perform_auto_scroll(app: &mut App, direction: i8) {
    if direction < 0 {
        let new_top = app.editor.visible_row_at_offset(app.editor.editor_scroll_top, -1);
        if new_top != app.editor.editor_scroll_top {
            app.editor.editor_scroll_top = new_top;
            app.editor.sync_scroll_offset();
            if app.state.config.editor.mode == EditingMode::Helix {
                app.editor.helix_move(CursorMove::Up, true);
            } else {
                app.editor.move_cursor(CursorMove::Up);
            }
        }
    } else {
        let new_top = app.editor.visible_row_at_offset(app.editor.editor_scroll_top, 1);
        if new_top != app.editor.editor_scroll_top {
            app.editor.editor_scroll_top = new_top;
            app.editor.sync_scroll_offset();
            if app.state.config.editor.mode == EditingMode::Helix {
                app.editor.helix_move(CursorMove::Down, true);
            } else {
                app.editor.move_cursor(CursorMove::Down);
            }
        }
    }
}

/// Move editor cursor to specific row/col position
pub(super) fn move_editor_cursor_to(app: &mut App, target_row: usize, target_col: usize) {
    app.editor.set_cursor_no_scroll(target_row, target_col);
}

pub(super) fn constrain_cursor_to_viewport(app: &mut App) {
    let view_height = app.editor.editor_view_height;
    if view_height == 0 {
        return;
    }
    let (cursor_row, cursor_col) = app.editor.cursor();
    let line_count = app.editor.line_count();
    let max_row = line_count.saturating_sub(1);
    let viewport_top = app.editor.editor_scroll_top;
    let viewport_bottom = app.editor.visible_row_at_offset(viewport_top, view_height.saturating_sub(1) as isize);
    let clamped_row = if cursor_row < viewport_top {
        viewport_top
    } else if cursor_row > viewport_bottom {
        viewport_bottom
    } else {
        cursor_row
    };
    let scrolloff = app.state.config.editor.scrolloff as usize;
    let effective_scrolloff = scrolloff.min(view_height / 2);
    let final_row = if effective_scrolloff > 0 && clamped_row == cursor_row {
        let scrolloff_top = app.editor.visible_row_at_offset(viewport_top, effective_scrolloff as isize);
        let scrolloff_bottom = app.editor.visible_row_at_offset(viewport_bottom, -(effective_scrolloff as isize));
        if cursor_row < scrolloff_top {
            scrolloff_top.min(max_row).min(viewport_bottom)
        } else if cursor_row > scrolloff_bottom {
            scrolloff_bottom.max(viewport_top)
        } else {
            cursor_row
        }
    } else {
        clamped_row
    };
    if app.state.config.editor.mode == EditingMode::Helix {
        if final_row != cursor_row {
            let head = app.editor.helix_offset(Position::new(final_row, cursor_col));
            let selection = match app.editor.helix_primary() {
                Some(primary) if app.editor.helix.mode == crate::helix::HelixMode::Select => crate::editor::HelixSelection { anchor: primary.anchor, head },
                _ => crate::editor::HelixSelection::caret(head),
            };
            app.editor.helix_set_selections(vec![selection], 0);
        }
        return;
    }
    app.editor.set_cursor_no_scroll(final_row, cursor_col);
}

const MENU_WIDTH: u16 = 14;

pub(super) fn get_context_menu_click(mouse_x: u16, mouse_y: u16, menu_x: u16, menu_y: u16) -> Option<ContextMenuItem> {
    let items = ContextMenuItem::all();
    let menu_height = items.len() as u16 + 2; // +2 for borders
    if mouse_x >= menu_x && mouse_x < menu_x + MENU_WIDTH && mouse_y >= menu_y && mouse_y < menu_y + menu_height {
        let relative_y = mouse_y.saturating_sub(menu_y).saturating_sub(1); // -1 for top border
        let index = relative_y as usize;
        if index < items.len() {
            return Some(items[index]);
        }
    }
    None
}

pub(super) fn get_context_menu_hover_index(mouse_x: u16, mouse_y: u16, menu_x: u16, menu_y: u16) -> Option<usize> {
    let items = ContextMenuItem::all();
    let menu_height = items.len() as u16 + 2;
    if mouse_x >= menu_x && mouse_x < menu_x + MENU_WIDTH && mouse_y > menu_y && mouse_y < menu_y + menu_height - 1 {
        let index = (mouse_y - menu_y - 1) as usize;
        if index < items.len() {
            return Some(index);
        }
    }
    None
}

pub(super) fn execute_context_menu_action(app: &mut App, action: ContextMenuItem) {
    if app.state.config.editor.mode == EditingMode::Helix {
        match action {
            ContextMenuItem::Copy => {
                app.editor.helix.selected_register = Some('+');
                yank(app);
            }
            ContextMenuItem::Cut => {
                app.editor.helix.selected_register = Some('+');
                yank(app);
                app.editor.helix_replace(&[String::new()], crate::editor::HelixRangeMode::Selection);
            }
            ContextMenuItem::Paste => paste_into_editor(app, None),
            ContextMenuItem::SelectAll => helix_select_all(app),
        }
        app.editor.context_menu_state = ContextMenuState::None;
        app.update_editor_block();
        return;
    }
    match action {
        ContextMenuItem::Copy => {
            app.editor.copy();
            app.editor.cancel_selection();
            if app.state.config.editor.mode == EditingMode::Vim {
                app.editor.vim.mode = VimMode::Normal;
                update_cursor_style(app);
            }
        }
        ContextMenuItem::Cut => {
            app.editor.cut();
            if app.state.config.editor.mode == EditingMode::Vim {
                app.editor.vim.mode = VimMode::Normal;
                update_cursor_style(app);
            }
        }
        ContextMenuItem::Paste => {
            paste_into_editor(app, None);
        }
        ContextMenuItem::SelectAll => {
            app.editor.select_all();
            if app.state.config.editor.mode == EditingMode::Vim {
                app.editor.vim.mode = VimMode::Visual;
                update_cursor_style(app);
            }
        }
    }
    app.update_editor_block();
}

fn handle_task_view_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(&(kind, _)) = app.tasks.filter_hits.iter().find(|(_, rect)| contains(*rect, mouse.column, mouse.row)) {
                app.tasks.text_input_active = false;
                app.cycle_task_filter(kind);
                return;
            }
            let Some(hit) = app.tasks.row_hits.iter().copied().find(|hit| contains(hit.row, mouse.column, mouse.row)) else {
                return;
            };
            app.tasks.text_input_active = false;
            app.task_select(hit.position);
            if contains(hit.checkbox, mouse.column, mouse.row) {
                app.toggle_task_from_view();
            }
        }
        MouseEventKind::ScrollUp if contains(app.tasks.list_area, mouse.column, mouse.row) => app.task_move_selection(-1),
        MouseEventKind::ScrollDown if contains(app.tasks.list_area, mouse.column, mouse.row) => app.task_move_selection(1),
        _ => {}
    }
}

fn contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x && column < rect.x.saturating_add(rect.width) && row >= rect.y && row < rect.y.saturating_add(rect.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppDependencies;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    struct SidebarApp {
        app: App,
        root: PathBuf,
    }

    impl SidebarApp {
        fn new() -> Self {
            let id = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("ekphos-sidebar-mouse-{}-{id}", std::process::id()));
            let vault = root.join("vault");
            fs::create_dir_all(&vault).unwrap();
            let alpha_path = vault.join("Alpha.md");
            fs::write(&alpha_path, "# Alpha\n\nAlpha body").unwrap();
            fs::write(vault.join("Beta.md"), "# Beta\n\nBeta body").unwrap();
            fs::write(vault.join("Board.canvas"), r#"{"nodes":[],"edges":[]}"#).unwrap();
            let config = Config { general: crate::config::GeneralConfig { welcome_shown: false, check_updates: false, ..Default::default() }, ..Default::default() };
            let dependencies = AppDependencies::headless(root.join("config"), root.join("cache"));
            let mut app = App::new_injected(config, vault, None, dependencies);
            app.state.show_welcome = false;
            app.state.show_changelog = false;
            app.state.dialog = DialogState::None;
            app.state.sidebar_area = Rect::new(0, 0, 30, 20);
            app.state.content_area = Rect::new(30, 0, 50, 20);
            assert!(app.select_note_by_path(&alpha_path));
            Self { app, root }
        }

        fn beta_sidebar_index(&self) -> usize {
            self.app
                .vault
                .sidebar_items
                .iter()
                .position(|item| match item.kind {
                    SidebarItemKind::Note { note_id } => self.app.vault.notes.iter().any(|note| note.id == note_id && note.title == "Beta"),
                    SidebarItemKind::Folder(_) => false,
                })
                .unwrap()
        }

        fn beta_click(&self, kind: MouseEventKind) -> crossterm::event::MouseEvent {
            crossterm::event::MouseEvent { kind, column: 1, row: self.beta_sidebar_index() as u16 + 1, modifiers: KeyModifiers::NONE }
        }
    }

    impl Drop for SidebarApp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn double_clicking_a_sidebar_note_opens_it_in_content() {
        let mut fixture = SidebarApp::new();
        let down = fixture.beta_click(MouseEventKind::Down(MouseButton::Left));
        let up = fixture.beta_click(MouseEventKind::Up(MouseButton::Left));

        handle_mouse_event(&mut fixture.app, down);
        handle_mouse_event(&mut fixture.app, up);
        handle_mouse_event(&mut fixture.app, down);

        assert_eq!(fixture.app.current_note().map(|note| note.title.as_str()), Some("Beta"));
        assert_eq!(fixture.app.current_body(), Some("# Beta\n\nBeta body"));
        assert_eq!(fixture.app.state.focus, Focus::Content);
    }

    #[test]
    fn enter_opens_highlighted_sidebar_note_when_document_selection_is_stale() {
        let mut fixture = SidebarApp::new();
        let beta_index = fixture.beta_sidebar_index();
        fixture.app.vault.selected_sidebar_index = beta_index;
        fixture.app.state.focus = Focus::Sidebar;
        assert_eq!(fixture.app.current_note().map(|note| note.title.as_str()), Some("Alpha"));

        let should_quit = handle_normal_mode(&mut fixture.app, crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(!should_quit);
        assert_eq!(fixture.app.current_note().map(|note| note.title.as_str()), Some("Beta"));
        assert_eq!(fixture.app.current_body(), Some("# Beta\n\nBeta body"));
        assert_eq!(fixture.app.state.focus, Focus::Content);
    }

    #[test]
    fn failed_sidebar_note_activation_keeps_keyboard_focus_in_sidebar() {
        let mut fixture = SidebarApp::new();
        let beta_index = fixture.beta_sidebar_index();
        let beta_path = fixture.app.vault.notes.iter().find(|note| note.title == "Beta").and_then(|note| note.file_path.clone()).unwrap();
        fs::remove_file(beta_path).unwrap();
        fixture.app.vault.selected_sidebar_index = beta_index;
        fixture.app.state.focus = Focus::Sidebar;

        execute_app_command(&mut fixture.app, AppCommand::Activate);

        assert_eq!(fixture.app.current_note().map(|note| note.title.as_str()), Some("Alpha"));
        assert_eq!(fixture.app.state.focus, Focus::Sidebar);
    }

    #[test]
    fn canvas_local_keys_leave_global_zen_binding_intact() {
        let mut fixture = SidebarApp::new();
        assert!(fixture.app.select_note_by_path(&fixture.root.join("vault/Board.canvas")));
        fixture.app.state.focus = Focus::Content;

        handle_normal_mode(&mut fixture.app, crossterm::event::KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
        assert!(fixture.app.state.zen_mode);

        handle_normal_mode(&mut fixture.app, crossterm::event::KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
        assert!(!fixture.app.state.zen_mode);
        handle_normal_mode(&mut fixture.app, crossterm::event::KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
        assert_eq!(fixture.app.state.status_message.as_deref(), Some("Nothing to undo"));
        handle_normal_mode(&mut fixture.app, crossterm::event::KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE));
        assert!(fixture.app.structured.canvas.shortcuts_expanded);

        let zoom = fixture.app.structured.canvas.zoom;
        handle_normal_mode(&mut fixture.app, crossterm::event::KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
        handle_normal_mode(&mut fixture.app, crossterm::event::KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert!(fixture.app.structured.canvas.zoom > zoom);
    }

    #[test]
    fn canvas_overlay_consumes_clicks_outside_the_content_panel() {
        let mut fixture = SidebarApp::new();
        assert!(fixture.app.select_note_by_path(&fixture.root.join("vault/Board.canvas")));
        fixture.app.state.focus = Focus::Content;
        fixture.app.structured.canvas.view_area = fixture.app.state.content_area;
        assert!(fixture.app.canvas_open_context_menu(ratatui::layout::Position::new(40, 5), crate::app::CanvasMenuTarget::Background));

        let sidebar_click = fixture.beta_click(MouseEventKind::Down(MouseButton::Left));
        handle_mouse_event(&mut fixture.app, sidebar_click);

        assert_eq!(fixture.app.current_note().map(|note| note.title.as_str()), Some("Board"));
        assert_eq!(fixture.app.state.focus, Focus::Content);
        assert!(!fixture.app.canvas_overlay_active());
    }

    #[test]
    fn canvas_redo_does_not_run_behind_an_open_overlay() {
        let mut fixture = SidebarApp::new();
        assert!(fixture.app.select_note_by_path(&fixture.root.join("vault/Board.canvas")));
        fixture.app.state.focus = Focus::Content;
        fixture.app.structured.canvas.view_area = fixture.app.state.content_area;
        let document = fixture.app.structured.canvas.document.clone().unwrap();
        fixture.app.structured.canvas.redo.push(document);
        assert!(fixture.app.canvas_open_add_menu());

        handle_normal_mode(&mut fixture.app, crossterm::event::KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));

        assert!(fixture.app.canvas_overlay_active());
        assert_eq!(fixture.app.structured.canvas.redo.len(), 1);
    }
}
