use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::canvas::{CanvasColor, CanvasEdge, CanvasEnd, CanvasNode, CanvasNodeKind, CanvasSide};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget, Wrap},
    Frame,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::panel::{panel_surface, render_panel, PanelFrame, SurfaceKind};
use crate::app::{App, CanvasEditorLayout, CanvasInteraction, CanvasMenuAction, CanvasMenuTarget, CanvasNodeEditor, CanvasOverlay, CanvasResizeHandle, Focus};
use crate::config::Theme;
use crate::keybindings::AppCommand;

const WORLD_UNITS_PER_COLUMN: f64 = 20.0;
const WORLD_UNITS_PER_ROW: f64 = 40.0;
const EDGE_NORTH: u8 = 1 << 0;
const EDGE_EAST: u8 = 1 << 1;
const EDGE_SOUTH: u8 = 1 << 2;
const EDGE_WEST: u8 = 1 << 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScreenRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl ScreenRect {
    fn right(self) -> i32 {
        self.x.saturating_add(self.width)
    }

    fn bottom(self) -> i32 {
        self.y.saturating_add(self.height)
    }

    fn center(self) -> (i32, i32) {
        (self.x.saturating_add(self.width / 2), self.y.saturating_add(self.height / 2))
    }

    fn clipped(self, area: Rect) -> Option<Rect> {
        let left = self.x.max(area.x as i32);
        let top = self.y.max(area.y as i32);
        let right = self.right().min(area.right() as i32);
        let bottom = self.bottom().min(area.bottom() as i32);
        (right > left && bottom > top).then(|| Rect::new(left as u16, top as u16, (right - left) as u16, (bottom - top) as u16))
    }
}

#[derive(Debug, Clone)]
struct RoutedEdge {
    points: Vec<(i32, i32)>,
    from_side: CanvasSide,
    to_side: CanvasSide,
}

#[derive(Debug, Clone, Copy)]
struct NodeAppearance {
    accent: Color,
    selected: bool,
    hovered: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct EdgeCell {
    directions: u8,
    color: Option<Color>,
    emphasized: bool,
}

struct EdgeLayer {
    area: Rect,
    cells: Vec<EdgeCell>,
}

impl EdgeLayer {
    fn new(area: Rect) -> Self {
        Self { area, cells: vec![EdgeCell::default(); area.width as usize * area.height as usize] }
    }

    fn add_path(&mut self, points: &[(i32, i32)], color: Color, emphasized: bool) {
        for segment in points.windows(2) {
            self.add_segment(segment[0], segment[1], color, emphasized);
        }
    }

    fn add_segment(&mut self, from: (i32, i32), to: (i32, i32), color: Color, emphasized: bool) {
        if from.1 == to.1 {
            let y = from.1;
            if y < self.area.y as i32 || y >= self.area.bottom() as i32 {
                return;
            }
            let start = from.0.min(to.0).max(self.area.x as i32);
            let end = from.0.max(to.0).min(self.area.right().saturating_sub(1) as i32);
            for x in start..end {
                self.connect((x, y), (x + 1, y), color, emphasized);
            }
        } else if from.0 == to.0 {
            let x = from.0;
            if x < self.area.x as i32 || x >= self.area.right() as i32 {
                return;
            }
            let start = from.1.min(to.1).max(self.area.y as i32);
            let end = from.1.max(to.1).min(self.area.bottom().saturating_sub(1) as i32);
            for y in start..end {
                self.connect((x, y), (x, y + 1), color, emphasized);
            }
        }
    }

    fn connect(&mut self, from: (i32, i32), to: (i32, i32), color: Color, emphasized: bool) {
        let (from_direction, to_direction) = match (to.0 - from.0, to.1 - from.1) {
            (1, 0) => (EDGE_EAST, EDGE_WEST),
            (-1, 0) => (EDGE_WEST, EDGE_EAST),
            (0, 1) => (EDGE_SOUTH, EDGE_NORTH),
            (0, -1) => (EDGE_NORTH, EDGE_SOUTH),
            _ => return,
        };
        self.add_direction(from, from_direction, color, emphasized);
        self.add_direction(to, to_direction, color, emphasized);
    }

    fn add_direction(&mut self, point: (i32, i32), direction: u8, color: Color, emphasized: bool) {
        if !contains(self.area, point) {
            return;
        }
        let x = point.0 as usize - self.area.x as usize;
        let y = point.1 as usize - self.area.y as usize;
        let index = y * self.area.width as usize + x;
        if let Some(cell) = self.cells.get_mut(index) {
            cell.directions |= direction;
            if emphasized || !cell.emphasized {
                cell.color = Some(color);
                cell.emphasized = emphasized;
            }
        }
    }

    fn render(&self, buffer: &mut Buffer) {
        for (index, edge) in self.cells.iter().enumerate() {
            if edge.directions == 0 {
                continue;
            }
            let x = self.area.x + (index % self.area.width as usize) as u16;
            let y = self.area.y + (index / self.area.width as usize) as u16;
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_char(edge_glyph(edge.directions));
                cell.set_style(Style::default().fg(edge.color.unwrap_or(Color::Reset)).add_modifier(if edge.emphasized { Modifier::BOLD } else { Modifier::empty() }));
            }
        }
    }
}

pub fn render_canvas_view(frame: &mut Frame, app: &mut App, area: Rect) {
    app.state.content_area = area;
    app.structured.canvas.node_rects.clear();
    app.structured.canvas.edge_cells.clear();
    app.structured.canvas.handle_rects.clear();
    app.structured.canvas.resize_rects.clear();
    app.structured.canvas.shortcut_toggle_rect = None;
    let theme = app.state.theme.clone();
    let focused = app.state.focus == Focus::Content;
    let note_title = app.current_note().map_or("Canvas", |note| note.title.as_str());
    let inner = if app.state.config.style.is_flat() {
        let panel = PanelFrame { style: app.state.config.style, theme: &theme, title: format!(" {note_title}.canvas "), focused, accent: theme.primary, surface: panel_surface(&app.state.config, &theme, SurfaceKind::Content) };
        render_panel(frame, &panel, area)
    } else {
        let border = if focused { theme.primary } else { theme.border };
        let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(border)).style(Style::default().bg(theme.background)).title(format!(" {note_title}.canvas "));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        inner
    };
    if inner.width < 12 || inner.height < 5 {
        frame.render_widget(Paragraph::new("Terminal too small for this Canvas").style(Style::default().fg(theme.muted).bg(theme.background)), inner);
        return;
    }
    if let Some(error) = app.structured.canvas.error.as_deref() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("Could not parse Canvas", Style::default().fg(theme.error).add_modifier(Modifier::BOLD))),
                Line::from(Span::styled(error, Style::default().fg(theme.muted))),
                Line::from(""),
                Line::from(Span::styled("Press E to edit the JSON source", Style::default().fg(theme.muted))),
            ])
            .style(Style::default().bg(theme.background))
            .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    let Some(document) = app.structured.canvas.document.clone() else {
        frame.render_widget(Paragraph::new("No Canvas available").style(Style::default().fg(theme.muted).bg(theme.background)), inner);
        return;
    };

    let desired_footer_height = if app.canvas_editor_active() {
        if inner.height >= 10 {
            2
        } else {
            1
        }
    } else if app.structured.canvas.shortcuts_expanded {
        if inner.height >= 12 {
            3
        } else {
            2
        }
    } else {
        1
    };
    let footer_height = desired_footer_height.min(inner.height.saturating_sub(1));
    let graph_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(1 + footer_height));
    app.structured.canvas.view_area = graph_area;
    fill_rect(frame.buffer_mut(), graph_area, theme.background);

    let mut editor_caret = None;
    if document.nodes.is_empty() {
        frame.render_widget(Paragraph::new("Double-click or right-click to add your first card · a opens the add menu").alignment(Alignment::Center).style(Style::default().fg(theme.muted).bg(theme.background)), graph_area);
    } else {
        fit_canvas_if_needed(app, &document, graph_area);
        let viewport = (app.structured.canvas.viewport_x, app.structured.canvas.viewport_y, app.structured.canvas.zoom);
        let screen_rects = document.nodes.iter().map(|node| node_screen_rect(node, graph_area, viewport)).collect::<Vec<_>>();
        let node_by_id = document.nodes.iter().enumerate().map(|(index, node)| (node.id.as_str(), index)).collect::<HashMap<_, _>>();

        for (index, node) in document.nodes.iter().enumerate() {
            if matches!(node.kind, CanvasNodeKind::Group { .. }) {
                let selected = index == app.structured.canvas.selected_node && app.structured.canvas.selected_edge.is_none();
                let hovered = app.structured.canvas.hovered_node == Some(index);
                let accent = node.color.as_ref().map_or(theme.border, |color| canvas_color(color, app));
                let editor = app.structured.canvas.editor.as_mut().filter(|editor| editor.node == index);
                editor_caret = draw_group(frame.buffer_mut(), node, screen_rects[index], graph_area, NodeAppearance { accent, selected, hovered }, editor, &theme).or(editor_caret);
            }
        }

        let mut edge_layer = EdgeLayer::new(graph_area);
        let mut edge_labels = Vec::new();
        let mut edge_ends = Vec::new();
        for (edge_index, edge) in document.edges.iter().enumerate() {
            let (Some(&from_index), Some(&to_index)) = (node_by_id.get(edge.from_node.as_str()), node_by_id.get(edge.to_node.as_str())) else {
                continue;
            };
            let route = route_edge(edge, screen_rects[from_index], screen_rects[to_index]);
            let selected = app.structured.canvas.selected_edge == Some(edge_index);
            let hovered = app.structured.canvas.hovered_edge == Some(edge_index);
            let connected = app.structured.canvas.selected_edge.is_none() && (from_index == app.structured.canvas.selected_node || to_index == app.structured.canvas.selected_node);
            let emphasized = selected || hovered || connected;
            let color = if emphasized { theme.primary } else { edge.color.as_ref().map_or(theme.border, |color| canvas_color(color, app)) };
            edge_layer.add_path(&route.points, color, emphasized);
            app.structured.canvas.edge_cells.extend(path_cells(&route.points, graph_area).into_iter().map(|position| (edge_index, position)));
            if let Some(label) = &edge.label {
                edge_labels.push((label.clone(), path_midpoint(&route.points), color, selected));
            }
            if edge.from_end == CanvasEnd::Arrow {
                edge_ends.push((outside_anchor(screen_rects[from_index], route.from_side), route.from_side, color, emphasized));
            }
            if edge.to_end == CanvasEnd::Arrow {
                edge_ends.push((outside_anchor(screen_rects[to_index], route.to_side), route.to_side, color, emphasized));
            }
        }

        add_connection_preview(app, &screen_rects, graph_area, &mut edge_layer, &mut edge_ends, theme.primary);
        edge_layer.render(frame.buffer_mut());
        for (point, side, color, emphasized) in edge_ends {
            draw_arrow(frame.buffer_mut(), point, side, graph_area, color, emphasized);
        }
        for (label, point, color, selected) in edge_labels {
            draw_edge_label(frame.buffer_mut(), &label, point, graph_area, color, selected, &theme);
        }

        let mut cards = document.nodes.iter().enumerate().filter(|(_, node)| !matches!(node.kind, CanvasNodeKind::Group { .. })).collect::<Vec<_>>();
        cards.sort_by_key(|(index, _)| *index == app.structured.canvas.selected_node);
        for (index, node) in cards {
            let screen_rect = screen_rects[index];
            let Some(clipped) = screen_rect.clipped(graph_area) else {
                continue;
            };
            let selected = index == app.structured.canvas.selected_node && app.structured.canvas.selected_edge.is_none();
            let hovered = app.structured.canvas.hovered_node == Some(index);
            let accent = node.color.as_ref().map_or(theme.info, |color| canvas_color(color, app));
            let editor = app.structured.canvas.editor.as_mut().filter(|editor| editor.node == index);
            editor_caret = draw_card(frame.buffer_mut(), node, screen_rect, graph_area, NodeAppearance { accent, selected, hovered }, editor, &theme).or(editor_caret);
            app.structured.canvas.node_rects.push((index, clipped));
        }
        for (index, _node) in document.nodes.iter().enumerate().filter(|(_, node)| matches!(node.kind, CanvasNodeKind::Group { .. })) {
            if let Some(clipped) = screen_rects[index].clipped(graph_area) {
                app.structured.canvas.node_rects.insert(0, (index, Rect::new(clipped.x, clipped.y, clipped.width, 1)));
            }
            if index == app.structured.canvas.selected_node && app.structured.canvas.selected_edge.is_none() {
                let allow_connections = app.structured.canvas.editor.is_none();
                draw_node_controls(app, frame.buffer_mut(), screen_rects[index], graph_area, theme.background, theme.primary, allow_connections);
            }
        }
        if let Some(node) = document.nodes.get(app.structured.canvas.selected_node) {
            if !matches!(node.kind, CanvasNodeKind::Group { .. }) && app.structured.canvas.selected_edge.is_none() {
                let allow_connections = app.structured.canvas.editor.is_none();
                draw_node_controls(app, frame.buffer_mut(), screen_rects[app.structured.canvas.selected_node], graph_area, theme.selection, theme.primary, allow_connections);
            }
        }
        if let CanvasInteraction::Connecting { from_node, from_side, pointer } = app.structured.canvas.interaction {
            if from_node != app.structured.canvas.selected_node {
                if let Some(source_rect) = screen_rects.get(from_node).copied() {
                    let target = pointer.map(|(x, y)| (x as i32, y as i32)).or_else(|| screen_rects.get(app.structured.canvas.selected_node).map(|rect| rect.center()));
                    let side = from_side.or_else(|| target.map(|target| screen_side_toward(source_rect, target))).unwrap_or(CanvasSide::Right);
                    let background = document.nodes.get(from_node).map_or(theme.background_secondary, |node| if matches!(node.kind, CanvasNodeKind::Group { .. }) { theme.background } else { theme.background_secondary });
                    draw_connection_source_handle(frame.buffer_mut(), source_rect, side, graph_area, background, theme.primary);
                }
            }
        }
    }

    if inner.height > 0 {
        render_top_status(frame, app, &document, Rect::new(inner.x, inner.y, inner.width, 1), &theme);
    }
    if footer_height > 0 {
        render_footer(frame, app, Rect::new(inner.x, inner.bottom().saturating_sub(footer_height), inner.width, footer_height), &theme);
    }
    render_canvas_overlay(frame, app, graph_area, &theme);
    if !app.canvas_overlay_active() {
        if let Some(caret) = editor_caret.filter(|caret| graph_area.contains(*caret)) {
            frame.set_cursor_position(caret);
        }
    }
}

fn render_canvas_overlay(frame: &mut Frame, app: &mut App, graph_area: Rect, theme: &Theme) {
    if graph_area.width == 0 || graph_area.height == 0 {
        match &mut app.structured.canvas.overlay {
            CanvasOverlay::Menu(menu) => {
                menu.area = Rect::default();
                menu.item_rects.clear();
            }
            CanvasOverlay::FilePicker(picker) => {
                picker.area = Rect::default();
                picker.result_rects.clear();
            }
            CanvasOverlay::None => {}
        }
        return;
    }
    match &app.structured.canvas.overlay {
        CanvasOverlay::None => {}
        CanvasOverlay::Menu(menu) => {
            let screen_position = menu.screen_position;
            let items = menu.items.clone();
            let selected_index = menu.selected_index;
            let target = menu.target;
            let width = items.iter().map(|action| canvas_menu_label(*action, target).width() as u16).max().unwrap_or(18).saturating_add(6).max(22);
            let height = items.len() as u16 + 2;
            let x = screen_position.x.min(frame.area().right().saturating_sub(width));
            let y = screen_position.y.min(frame.area().bottom().saturating_sub(height));
            let area = Rect::new(x, y, width.min(frame.area().width), height.min(frame.area().height));
            let item_rects = items.iter().enumerate().map(|(index, action)| (*action, Rect::new(area.x + 1, area.y + 1 + index as u16, area.width.saturating_sub(2), 1))).collect::<Vec<_>>();
            if let CanvasOverlay::Menu(menu) = &mut app.structured.canvas.overlay {
                menu.area = area;
                menu.item_rects.clone_from(&item_rects);
            }
            frame.render_widget(Clear, area);
            frame.render_widget(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme.primary)).style(Style::default().bg(theme.background_secondary)), area);
            for (index, (action, row)) in item_rects.iter().enumerate() {
                let selected = index == selected_index;
                let marker = if selected { "◆" } else { " " };
                render_overlay_row(frame, *row, format!(" {marker} {}", canvas_menu_label(*action, target)), selected, theme);
            }
        }
        CanvasOverlay::FilePicker(picker) => {
            const VISIBLE_RESULTS: usize = 10;
            let query = picker.query.clone();
            let results = picker.results.clone();
            let selected_index = picker.selected_index;
            let scroll_offset = picker.scroll_offset;
            let width = graph_area.width.saturating_sub(4).clamp(24, 64);
            let visible_count = results.len().clamp(1, VISIBLE_RESULTS);
            let height = (visible_count as u16 + 4).min(graph_area.height.max(1));
            let area = centered_rect(graph_area, width, height);
            let results_area = Rect::new(area.x + 1, area.y + 3, area.width.saturating_sub(2), area.height.saturating_sub(4));
            let result_rects = results.iter().enumerate().skip(scroll_offset).take(results_area.height as usize).map(|(index, _)| (index, Rect::new(results_area.x, results_area.y + (index - scroll_offset) as u16, results_area.width, 1))).collect::<Vec<_>>();
            if let CanvasOverlay::FilePicker(picker) = &mut app.structured.canvas.overlay {
                picker.area = area;
                picker.result_rects.clone_from(&result_rects);
            }
            frame.render_widget(Clear, area);
            let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(theme.primary)).style(Style::default().bg(theme.background_secondary)).title(" Add file card ");
            frame.render_widget(block, area);
            let query_area = Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(4), 1);
            frame.render_widget(Paragraph::new(format!("Search: {query}")).style(Style::default().fg(theme.foreground).bg(theme.background_secondary)), query_area);
            frame.render_widget(Paragraph::new("Enter add · Esc cancel").style(Style::default().fg(theme.muted).bg(theme.background_secondary)), Rect::new(area.x + 2, area.y + 2, area.width.saturating_sub(4), 1));
            if results.is_empty() {
                frame.render_widget(Paragraph::new("No matching vault files").style(Style::default().fg(theme.muted).bg(theme.background_secondary)), results_area);
            } else {
                for ((index, result), (_, row)) in results.iter().enumerate().skip(scroll_offset).take(results_area.height as usize).zip(&result_rects) {
                    let selected = index == selected_index;
                    let marker = if selected { "◆" } else { " " };
                    let hint = result.folder_hint.as_deref().map(|folder| format!(" — {folder}")).unwrap_or_default();
                    render_overlay_row(frame, *row, format!(" {marker} {}{hint}", result.display_name), selected, theme);
                }
            }
            let query_width = query.width() as u16;
            if query_area.width > 0 && query_area.height > 0 {
                frame.set_cursor_position(Position::new((query_area.x + "Search: ".width() as u16 + query_width).min(query_area.right().saturating_sub(1)), query_area.y));
            }
        }
    }
}

fn render_overlay_row(frame: &mut Frame, area: Rect, label: String, selected: bool, theme: &Theme) {
    let style = if selected { Style::default().fg(theme.background).bg(theme.primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.foreground).bg(theme.background_secondary) };
    fill_rect(frame.buffer_mut(), area, if selected { theme.primary } else { theme.background_secondary });
    frame.render_widget(Paragraph::new(label).style(style), area);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(area.x + area.width.saturating_sub(width) / 2, area.y + area.height.saturating_sub(height) / 2, width, height)
}

fn canvas_menu_label(action: CanvasMenuAction, target: CanvasMenuTarget) -> &'static str {
    match action {
        CanvasMenuAction::AddText => "Add text card",
        CanvasMenuAction::AddFile => "Add file card",
        CanvasMenuAction::AddLink => "Add link card",
        CanvasMenuAction::AddGroup => "Add group",
        CanvasMenuAction::Edit => "Edit card",
        CanvasMenuAction::RenameGroup => "Rename group",
        CanvasMenuAction::Open => "Open linked item",
        CanvasMenuAction::Duplicate => "Duplicate card",
        CanvasMenuAction::Connect => "Connect card",
        CanvasMenuAction::Delete if matches!(target, CanvasMenuTarget::Edge(_)) => "Delete connection",
        CanvasMenuAction::Delete => "Delete card",
        CanvasMenuAction::Fit => "Fit Canvas to view",
    }
}

fn fit_canvas_if_needed(app: &mut App, document: &crate::canvas::Canvas, graph_area: Rect) {
    if !app.structured.canvas.needs_fit {
        return;
    }
    if let Some((min_x, min_y, max_x, max_y)) = document.bounds() {
        let width = (max_x - min_x).max(1) as f64;
        let height = (max_y - min_y).max(1) as f64;
        let zoom = ((graph_area.width as f64 * WORLD_UNITS_PER_COLUMN / width).min(graph_area.height as f64 * WORLD_UNITS_PER_ROW / height) * 0.84).clamp(0.1, 8.0);
        let center_x = (min_x + max_x) as f64 / 2.0;
        let center_y = (min_y + max_y) as f64 / 2.0;
        app.structured.canvas.zoom = zoom;
        app.structured.canvas.viewport_x = center_x - graph_area.width as f64 * WORLD_UNITS_PER_COLUMN / zoom / 2.0;
        app.structured.canvas.viewport_y = center_y - graph_area.height as f64 * WORLD_UNITS_PER_ROW / zoom / 2.0;
    }
    app.structured.canvas.needs_fit = false;
}

fn render_top_status(frame: &mut Frame, app: &App, document: &crate::canvas::Canvas, area: Rect, theme: &Theme) {
    let zoom = (app.structured.canvas.zoom * 100.0).round() as i32;
    let mut spans = vec![Span::styled(format!(" {} nodes", document.nodes.len()), Style::default().fg(theme.foreground).add_modifier(Modifier::BOLD)), Span::styled(format!(" · {} connections · {zoom}%", document.edges.len()), Style::default().fg(theme.muted))];
    if let Some(editor) = app.structured.canvas.editor.as_ref() {
        let position = if editor.field.multiline() { format!("row {}/{}", editor.caret_row.saturating_add(1), editor.total_rows.max(1)) } else { format!("column {}/{}", editor.caret_column.saturating_add(1), editor.draft.width().saturating_add(1)) };
        spans.push(Span::styled(format!("  ◆ Editing {} · {position}", editor.field.label()), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)));
    } else {
        match app.structured.canvas.interaction {
            CanvasInteraction::Connecting { .. } => spans.push(Span::styled("  ◆ Connecting · choose a target", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD))),
            CanvasInteraction::ResizingNode { .. } => spans.push(Span::styled("  ◇ Resizing · Shift keeps aspect ratio", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD))),
            _ => {
                if let Some(edge_index) = app.structured.canvas.selected_edge {
                    if let Some(edge) = document.edges.get(edge_index) {
                        let label = edge.label.as_deref().map_or_else(|| format!("{} → {}", edge.from_node, edge.to_node), |label| label.to_string());
                        spans.push(Span::styled(format!("  ◆ {label}"), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)));
                    }
                } else if let Some(node) = document.nodes.get(app.structured.canvas.selected_node) {
                    spans.push(Span::styled(format!("  ◆ {}", node_summary(node)), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)));
                }
            }
        }
    }
    if !app.structured.canvas.diagnostics.is_empty() {
        spans.push(Span::styled(format!("  ⚠ {} issue(s)", app.structured.canvas.diagnostics.len()), Style::default().fg(theme.warning)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.background)), area);
}

fn render_footer(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    app.structured.canvas.shortcut_toggle_rect = None;
    if let Some(editor) = app.structured.canvas.editor.as_ref() {
        let save_key = if editor.field.multiline() { "Ctrl+S" } else { "Enter" };
        let first = Line::from(vec![key(save_key, theme), hint(" save  ", theme), key("Esc", theme), hint(" cancel  ", theme), key("Arrows", theme), hint(" move caret  ", theme), key("Home/End", theme), hint(" row", theme)]);
        frame.render_widget(Paragraph::new(first).style(Style::default().bg(theme.background)), Rect::new(area.x, area.y, area.width, 1));
        if area.height > 1 {
            let second = if editor.field.multiline() {
                Line::from(vec![key("Enter", theme), hint(" new line  ", theme), key("PgUp/PgDn", theme), hint(" page  ", theme), key("Ctrl+Home/End", theme), hint(" document  ", theme), key("◇ drag", theme), hint(" resize", theme)])
            } else {
                Line::from(vec![key("Paste", theme), hint(" insert  ", theme), key("◀/▶", theme), hint(" hidden text  ", theme), key("◇ drag", theme), hint(" resize  ", theme), key("Click outside", theme), hint(" save", theme)])
            };
            frame.render_widget(Paragraph::new(second).style(Style::default().bg(theme.background)), Rect::new(area.x, area.y + 1, area.width, 1));
        }
        return;
    }
    let expanded = app.structured.canvas.shortcuts_expanded;
    let toggle_key = app.state.keymap.binding_label(AppCommand::ToggleCanvasShortcuts);
    let toggle_text = if expanded { format!("{toggle_key} hide ▲") } else { format!("{toggle_key} shortcuts ▼") };
    let toggle_width = (toggle_text.width() as u16 + 2).min(area.width);
    let toggle_area = Rect::new(area.right().saturating_sub(toggle_width), area.y, toggle_width, 1);
    app.structured.canvas.shortcut_toggle_rect = Some(toggle_area);
    let toggle_style = if app.structured.canvas.shortcut_toggle_hovered { Style::default().fg(theme.background).bg(theme.primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.warning).bg(theme.background).add_modifier(Modifier::BOLD) };
    frame.render_widget(Paragraph::new(format!(" {toggle_text} ")).alignment(Alignment::Right).style(toggle_style), toggle_area);

    let summary_area = Rect::new(area.x, area.y, area.width.saturating_sub(toggle_width.saturating_add(1)), 1);
    let connecting = matches!(app.structured.canvas.interaction, CanvasInteraction::Connecting { .. });
    let summary = if connecting {
        Line::from(vec![key("Enter", theme), hint(" attach  ", theme), key("Esc", theme), hint(" cancel  ", theme), key("Arrows/HJKL", theme), hint(" choose target", theme)])
    } else if summary_area.width < 45 {
        Line::from(vec![key("a", theme), hint(" add  ", theme), key("Right-click", theme), hint(" menu", theme)])
    } else {
        Line::from(vec![key("a", theme), hint(" add  ", theme), key("Right-click", theme), hint(" actions  ", theme), key("Drag", theme), hint(" move/pan  ", theme), key("Scroll", theme), hint(" zoom", theme)])
    };
    frame.render_widget(Paragraph::new(summary).style(Style::default().bg(theme.background)), summary_area);
    if !expanded || area.height <= 1 {
        return;
    }

    let horizontal_keys = format!("{}/{}", app.state.keymap.binding_label(AppCommand::CanvasSelectLeft), app.state.keymap.binding_label(AppCommand::CanvasSelectRight));
    let zoom_keys = format!("{}/{}", app.state.keymap.binding_label(AppCommand::CanvasZoomIn), app.state.keymap.binding_label(AppCommand::CanvasZoomOut));
    let navigation = if area.width < 86 {
        Line::from(vec![key("j/k/↑/↓", theme), hint(" vertical  ", theme), owned_key(horizontal_keys, theme), hint(" horizontal  ", theme), owned_key(zoom_keys, theme), hint(" zoom  ", theme), key("f", theme), hint(" fit", theme)])
    } else {
        Line::from(vec![
            key("j/k/↑/↓", theme),
            hint(" vertical  ", theme),
            owned_key(horizontal_keys, theme),
            hint(" horizontal  ", theme),
            key("Shift+arrows", theme),
            hint(" pan  ", theme),
            owned_key(zoom_keys, theme),
            hint(" zoom  ", theme),
            key("f", theme),
            hint(" fit  ", theme),
            key("Enter/Space", theme),
            hint(" edit/open  ", theme),
            key("Shift+F10", theme),
            hint(" menu", theme),
        ])
    };
    frame.render_widget(Paragraph::new(navigation).style(Style::default().bg(theme.background)), Rect::new(area.x, area.y + 1, area.width, 1));
    if area.height > 2 {
        let undo_key = app.state.keymap.binding_label(AppCommand::CanvasUndo);
        let redo_key = app.state.keymap.binding_label(AppCommand::CanvasRedo);
        let actions = Line::from(vec![
            key("◇ drag", theme),
            hint(" resize  ", theme),
            key("● drag/c", theme),
            hint(" connect  ", theme),
            key("[ ]", theme),
            hint(" connections  ", theme),
            key("Del", theme),
            hint(" delete  ", theme),
            owned_key(undo_key, theme),
            hint(" undo  ", theme),
            owned_key(redo_key, theme),
            hint(" redo  ", theme),
            key("E", theme),
            hint(" source", theme),
        ]);
        frame.render_widget(Paragraph::new(actions).style(Style::default().bg(theme.background)), Rect::new(area.x, area.y + 2, area.width, 1));
    }
}

fn key(text: &'static str, theme: &Theme) -> Span<'static> {
    Span::styled(text, Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))
}

fn owned_key(text: String, theme: &Theme) -> Span<'static> {
    Span::styled(text, Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))
}

fn hint(text: &'static str, theme: &Theme) -> Span<'static> {
    Span::styled(text, Style::default().fg(theme.muted))
}

fn node_screen_rect(node: &CanvasNode, area: Rect, viewport: (f64, f64, f64)) -> ScreenRect {
    let scale = viewport.2;
    let x = area.x as i32 + ((node.x as f64 - viewport.0) * scale / WORLD_UNITS_PER_COLUMN).round() as i32;
    let y = area.y as i32 + ((node.y as f64 - viewport.1) * scale / WORLD_UNITS_PER_ROW).round() as i32;
    let width = (node.width.max(1) as f64 * scale / WORLD_UNITS_PER_COLUMN).round().clamp(5.0, 4096.0) as i32;
    let height = (node.height.max(1) as f64 * scale / WORLD_UNITS_PER_ROW).round().clamp(3.0, 2048.0) as i32;
    ScreenRect { x, y, width, height }
}

fn draw_card(buffer: &mut Buffer, node: &CanvasNode, rect: ScreenRect, clip_area: Rect, appearance: NodeAppearance, editor: Option<&mut CanvasNodeEditor>, theme: &Theme) -> Option<Position> {
    let clipped = rect.clipped(clip_area)?;
    let surface = if appearance.selected { theme.selection } else { theme.background_secondary };
    let border = if appearance.selected { theme.primary } else { appearance.accent };
    fill_rect(buffer, clipped, surface);
    if let Some(editor) = editor {
        draw_visible_border(buffer, rect, clip_area, border, surface, true);
        let marker = if appearance.selected { "◆ " } else { "" };
        let title = format!(" {marker}Editing {} ", editor.field.label());
        draw_inline_text(buffer, &title, (rect.x.saturating_add(2), rect.y), clip_area, Style::default().fg(border).bg(surface).add_modifier(Modifier::BOLD), rect.width.saturating_sub(4).max(0) as usize);
        let body = ScreenRect { x: rect.x.saturating_add(1), y: rect.y.saturating_add(1), width: rect.width.saturating_sub(2), height: rect.height.saturating_sub(2) }.clipped(clip_area).unwrap_or_default();
        let layout = editor.layout(body);
        render_editor_layout(buffer, &layout, body, clip_area, surface, theme)
    } else {
        let max_width = (clip_area.width as i32 * 4).max(80);
        let max_height = (clip_area.height as i32 * 4).max(40);
        if rect.width > max_width || rect.height > max_height {
            draw_visible_border(buffer, rect, clip_area, border, surface, appearance.selected || appearance.hovered);
            return None;
        }

        let local_area = Rect::new(0, 0, rect.width as u16, rect.height as u16);
        let mut local = Buffer::empty(local_area);
        local.set_style(local_area, Style::default().fg(theme.foreground).bg(surface));
        let marker = if appearance.selected { "◆ " } else { "" };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border).bg(surface).add_modifier(if appearance.selected || appearance.hovered { Modifier::BOLD } else { Modifier::empty() }))
            .style(Style::default().fg(theme.foreground).bg(surface))
            .title(Span::styled(format!(" {marker}{} ", node_type_label(node)), Style::default().fg(border).bg(surface).add_modifier(Modifier::BOLD)));
        Paragraph::new(card_text(node, appearance.accent, theme)).style(Style::default().fg(theme.foreground).bg(surface)).wrap(Wrap { trim: true }).block(block).render(local_area, &mut local);

        for y in clipped.y..clipped.bottom() {
            for x in clipped.x..clipped.right() {
                let local_x = (x as i32 - rect.x) as u16;
                let local_y = (y as i32 - rect.y) as u16;
                if let (Some(source), Some(destination)) = (local.cell((local_x, local_y)), buffer.cell_mut((x, y))) {
                    *destination = source.clone();
                }
            }
        }
        None
    }
}

fn card_text(node: &CanvasNode, accent: Color, theme: &Theme) -> Text<'static> {
    match &node.kind {
        CanvasNodeKind::Text { text } => Text::from(
            text.lines()
                .map(|line| {
                    let trimmed = line.trim_start();
                    let heading = trimmed.chars().take_while(|character| *character == '#').count();
                    if (1..=6).contains(&heading) && trimmed.as_bytes().get(heading) == Some(&b' ') {
                        Line::from(Span::styled(trimmed[heading + 1..].to_string(), Style::default().fg(accent).add_modifier(Modifier::BOLD)))
                    } else {
                        Line::from(line.to_string())
                    }
                })
                .collect::<Vec<_>>(),
        ),
        CanvasNodeKind::File { file, subpath } => {
            let name = Path::new(file).file_name().and_then(|name| name.to_str()).unwrap_or(file);
            let mut lines = vec![Line::from(Span::styled(name.to_string(), Style::default().fg(theme.foreground).add_modifier(Modifier::BOLD)))];
            if name != file {
                lines.push(Line::from(Span::styled(file.clone(), Style::default().fg(theme.muted))));
            }
            if let Some(subpath) = subpath {
                lines.push(Line::from(Span::styled(subpath.clone(), Style::default().fg(accent))));
            }
            Text::from(lines)
        }
        CanvasNodeKind::Link { url } => Text::from(vec![Line::from(Span::styled("Open link", Style::default().fg(theme.foreground).add_modifier(Modifier::BOLD))), Line::from(Span::styled(url.clone(), Style::default().fg(accent)))]),
        CanvasNodeKind::Unknown { kind } => Text::from(vec![Line::from(Span::styled(format!("Unsupported node: {kind}"), Style::default().fg(theme.warning)))]),
        CanvasNodeKind::Group { label, .. } => Text::from(label.clone().unwrap_or_else(|| "Group".to_string())),
    }
}

fn draw_visible_border(buffer: &mut Buffer, rect: ScreenRect, area: Rect, color: Color, background: Color, bold: bool) {
    let style = Style::default().fg(color).bg(background).add_modifier(if bold { Modifier::BOLD } else { Modifier::empty() });
    for x in rect.x + 1..rect.right() - 1 {
        put(buffer, (x, rect.y), '─', area, style);
        put(buffer, (x, rect.bottom() - 1), '─', area, style);
    }
    for y in rect.y + 1..rect.bottom() - 1 {
        put(buffer, (rect.x, y), '│', area, style);
        put(buffer, (rect.right() - 1, y), '│', area, style);
    }
    put(buffer, (rect.x, rect.y), '╭', area, style);
    put(buffer, (rect.right() - 1, rect.y), '╮', area, style);
    put(buffer, (rect.x, rect.bottom() - 1), '╰', area, style);
    put(buffer, (rect.right() - 1, rect.bottom() - 1), '╯', area, style);
}

fn draw_group(buffer: &mut Buffer, node: &CanvasNode, rect: ScreenRect, area: Rect, appearance: NodeAppearance, editor: Option<&mut CanvasNodeEditor>, theme: &Theme) -> Option<Position> {
    if rect.width < 3 || rect.height < 2 || rect.clipped(area).is_none() {
        return None;
    }
    let color = if appearance.selected { theme.primary } else { appearance.accent };
    let style = Style::default().fg(color).bg(theme.background).add_modifier(if appearance.selected || appearance.hovered { Modifier::BOLD } else { Modifier::DIM });
    let horizontal = if appearance.selected { '─' } else { '┄' };
    let vertical = if appearance.selected { '│' } else { '┆' };
    for x in rect.x + 1..rect.right() - 1 {
        put(buffer, (x, rect.y), horizontal, area, style);
        put(buffer, (x, rect.bottom() - 1), horizontal, area, style);
    }
    for y in rect.y + 1..rect.bottom() - 1 {
        put(buffer, (rect.x, y), vertical, area, style);
        put(buffer, (rect.right() - 1, y), vertical, area, style);
    }
    for (point, glyph) in [((rect.x, rect.y), '╭'), ((rect.right() - 1, rect.y), '╮'), ((rect.x, rect.bottom() - 1), '╰'), ((rect.right() - 1, rect.bottom() - 1), '╯')] {
        put(buffer, point, glyph, area, style);
    }
    let label = match &node.kind {
        CanvasNodeKind::Group { label, .. } => label.as_deref().unwrap_or("Group"),
        _ => "Group",
    };
    if let Some(editor) = editor {
        let edit_area = ScreenRect { x: rect.x.saturating_add(1), y: rect.y, width: rect.width.saturating_sub(2), height: 1 }.clipped(area).unwrap_or_default();
        fill_rect(buffer, edit_area, theme.background_secondary);
        let layout = editor.layout(edit_area);
        render_editor_layout(buffer, &layout, edit_area, area, theme.background_secondary, theme)
    } else {
        let prefix = if appearance.selected { "◆ " } else { "" };
        draw_inline_text(buffer, &format!(" {prefix}{label} "), (rect.x + 2, rect.y), area, style, (rect.width - 4).max(0) as usize);
        None
    }
}

fn render_editor_layout(buffer: &mut Buffer, layout: &CanvasEditorLayout, editor_area: Rect, clip_area: Rect, surface: Color, theme: &Theme) -> Option<Position> {
    let text_style = Style::default().fg(theme.foreground).bg(surface);
    for row in &layout.rows {
        draw_inline_text(buffer, &row.text, (row.area.x as i32, row.area.y as i32), clip_area, text_style, row.area.width as usize);
    }
    if layout.multiline && editor_area.width >= 2 && editor_area.height > 0 {
        let rail_x = editor_area.right().saturating_sub(1);
        if editor_area.height == 1 && layout.hidden_before && layout.hidden_after {
            put(buffer, (rail_x as i32, editor_area.y as i32), '↕', clip_area, overflow_style(layout.caret_before || layout.caret_after, surface, theme));
        } else {
            if layout.hidden_before {
                put(buffer, (rail_x as i32, editor_area.y as i32), '▲', clip_area, overflow_style(layout.caret_before, surface, theme));
            }
            if layout.hidden_after {
                put(buffer, (rail_x as i32, editor_area.bottom().saturating_sub(1) as i32), '▼', clip_area, overflow_style(layout.caret_after, surface, theme));
            }
        }
    } else if !layout.multiline && editor_area.width >= 3 && editor_area.height > 0 {
        if layout.hidden_before {
            put(buffer, (editor_area.x as i32, editor_area.y as i32), '◀', clip_area, overflow_style(layout.caret_before, surface, theme));
        }
        if layout.hidden_after {
            put(buffer, (editor_area.right().saturating_sub(1) as i32, editor_area.y as i32), '▶', clip_area, overflow_style(layout.caret_after, surface, theme));
        }
    }
    if let Some(caret) = layout.caret {
        if let Some(cell) = buffer.cell_mut(caret) {
            cell.set_style(Style::default().fg(theme.foreground).bg(surface).add_modifier(Modifier::BOLD | Modifier::REVERSED));
        }
    }
    layout.caret
}

fn overflow_style(points_to_caret: bool, surface: Color, theme: &Theme) -> Style {
    Style::default().fg(if points_to_caret { theme.primary } else { theme.muted }).bg(surface).add_modifier(if points_to_caret { Modifier::BOLD } else { Modifier::empty() })
}

fn draw_node_controls(app: &mut App, buffer: &mut Buffer, rect: ScreenRect, area: Rect, background: Color, color: Color, allow_connections: bool) {
    for handle in [CanvasResizeHandle::TopLeft, CanvasResizeHandle::TopRight, CanvasResizeHandle::BottomRight, CanvasResizeHandle::BottomLeft, CanvasResizeHandle::Top, CanvasResizeHandle::Right, CanvasResizeHandle::Bottom, CanvasResizeHandle::Left] {
        if let Some(target) = resize_target(rect, area, handle) {
            app.structured.canvas.resize_rects.push((handle, target));
        }
    }
    for point in [(rect.x, rect.y), (rect.right() - 1, rect.y), (rect.right() - 1, rect.bottom() - 1), (rect.x, rect.bottom() - 1)] {
        put(buffer, point, '◇', area, Style::default().fg(color).bg(background).add_modifier(Modifier::BOLD));
    }
    if !allow_connections {
        draw_resize_hover(app, buffer, area, background, color);
        return;
    }
    let active_side = match app.structured.canvas.interaction {
        CanvasInteraction::Connecting { from_node, from_side, .. } if from_node == app.structured.canvas.selected_node => from_side,
        _ => None,
    };
    for side in [CanvasSide::Top, CanvasSide::Right, CanvasSide::Bottom, CanvasSide::Left] {
        let point = border_anchor(rect, side);
        if !contains(area, point) {
            continue;
        }
        let glyph = if active_side == Some(side) { '◆' } else { '●' };
        put(buffer, point, glyph, area, Style::default().fg(color).bg(background).add_modifier(Modifier::BOLD));
        app.structured.canvas.handle_rects.push((side, Rect::new(point.0 as u16, point.1 as u16, 1, 1)));
    }
    draw_resize_hover(app, buffer, area, background, color);
}

fn draw_resize_hover(app: &App, buffer: &mut Buffer, area: Rect, background: Color, color: Color) {
    let active = match app.structured.canvas.interaction {
        CanvasInteraction::ResizingNode { handle, last, .. } => Some((handle, Position::new(last.0, last.1))),
        _ => app.structured.canvas.hovered_resize,
    };
    if let Some((handle, pointer)) = active.filter(|(_, pointer)| area.contains(*pointer)) {
        put(buffer, (pointer.x as i32, pointer.y as i32), handle.glyph(), area, Style::default().fg(color).bg(background).add_modifier(Modifier::BOLD));
    }
}

fn resize_target(rect: ScreenRect, area: Rect, handle: CanvasResizeHandle) -> Option<Rect> {
    let target = match handle {
        CanvasResizeHandle::TopLeft => ScreenRect { x: rect.x, y: rect.y, width: 1, height: 1 },
        CanvasResizeHandle::TopRight => ScreenRect { x: rect.right() - 1, y: rect.y, width: 1, height: 1 },
        CanvasResizeHandle::BottomRight => ScreenRect { x: rect.right() - 1, y: rect.bottom() - 1, width: 1, height: 1 },
        CanvasResizeHandle::BottomLeft => ScreenRect { x: rect.x, y: rect.bottom() - 1, width: 1, height: 1 },
        CanvasResizeHandle::Top => ScreenRect { x: rect.x + 1, y: rect.y, width: rect.width.saturating_sub(2), height: 1 },
        CanvasResizeHandle::Right => ScreenRect { x: rect.right() - 1, y: rect.y + 1, width: 1, height: rect.height.saturating_sub(2) },
        CanvasResizeHandle::Bottom => ScreenRect { x: rect.x + 1, y: rect.bottom() - 1, width: rect.width.saturating_sub(2), height: 1 },
        CanvasResizeHandle::Left => ScreenRect { x: rect.x, y: rect.y + 1, width: 1, height: rect.height.saturating_sub(2) },
    };
    target.clipped(area)
}

fn draw_connection_source_handle(buffer: &mut Buffer, rect: ScreenRect, side: CanvasSide, area: Rect, background: Color, color: Color) {
    let point = border_anchor(rect, side);
    put(buffer, point, '◆', area, Style::default().fg(color).bg(background).add_modifier(Modifier::BOLD));
}

fn route_edge(edge: &CanvasEdge, from: ScreenRect, to: ScreenRect) -> RoutedEdge {
    let from_side = edge.from_side.unwrap_or_else(|| screen_side_toward(from, to.center()));
    let to_side = edge.to_side.unwrap_or_else(|| screen_side_toward(to, from.center()));
    let start = outside_anchor(from, from_side);
    let end = outside_anchor(to, to_side);
    RoutedEdge { points: orthogonal_route(start, end, from_side, to_side), from_side, to_side }
}

fn orthogonal_route(start: (i32, i32), end: (i32, i32), from_side: CanvasSide, to_side: CanvasSide) -> Vec<(i32, i32)> {
    let from_horizontal = matches!(from_side, CanvasSide::Left | CanvasSide::Right);
    let to_horizontal = matches!(to_side, CanvasSide::Left | CanvasSide::Right);
    let points = if from_horizontal && to_horizontal {
        let middle = start.0 + (end.0 - start.0) / 2;
        vec![start, (middle, start.1), (middle, end.1), end]
    } else if !from_horizontal && !to_horizontal {
        let middle = start.1 + (end.1 - start.1) / 2;
        vec![start, (start.0, middle), (end.0, middle), end]
    } else if from_horizontal {
        vec![start, (end.0, start.1), end]
    } else {
        vec![start, (start.0, end.1), end]
    };
    simplify_path(points)
}

fn simplify_path(points: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
    let mut simplified = Vec::new();
    for point in points {
        if simplified.last() == Some(&point) {
            continue;
        }
        if simplified.len() >= 2 {
            let first = simplified[simplified.len() - 2];
            let second = simplified[simplified.len() - 1];
            if (first.0 == second.0 && second.0 == point.0) || (first.1 == second.1 && second.1 == point.1) {
                simplified.pop();
            }
        }
        simplified.push(point);
    }
    simplified
}

fn add_connection_preview(app: &App, rects: &[ScreenRect], area: Rect, layer: &mut EdgeLayer, ends: &mut Vec<((i32, i32), CanvasSide, Color, bool)>, color: Color) {
    let CanvasInteraction::Connecting { from_node, from_side, pointer } = app.structured.canvas.interaction else {
        return;
    };
    let Some(from_rect) = rects.get(from_node).copied() else {
        return;
    };
    let (end, target_side, source_side) = if let Some(pointer) = pointer {
        let end = (pointer.0.clamp(area.x, area.right().saturating_sub(1)) as i32, pointer.1.clamp(area.y, area.bottom().saturating_sub(1)) as i32);
        let source_side = from_side.unwrap_or_else(|| screen_side_toward(from_rect, end));
        (end, screen_side_toward_point(end, from_rect.center()), source_side)
    } else {
        let target_index = app.structured.canvas.selected_node;
        if target_index == from_node {
            return;
        }
        let Some(target_rect) = rects.get(target_index).copied() else {
            return;
        };
        let source_side = from_side.unwrap_or_else(|| screen_side_toward(from_rect, target_rect.center()));
        let target_side = screen_side_toward(target_rect, from_rect.center());
        (outside_anchor(target_rect, target_side), target_side, source_side)
    };
    let start = outside_anchor(from_rect, source_side);
    let points = orthogonal_route(start, end, source_side, target_side);
    layer.add_path(&points, color, true);
    ends.push((end, target_side, color, true));
}

fn screen_side_toward(rect: ScreenRect, point: (i32, i32)) -> CanvasSide {
    let center = rect.center();
    let dx = point.0 - center.0;
    let dy = point.1 - center.1;
    if dx.abs() >= dy.abs() {
        if dx >= 0 {
            CanvasSide::Right
        } else {
            CanvasSide::Left
        }
    } else if dy >= 0 {
        CanvasSide::Bottom
    } else {
        CanvasSide::Top
    }
}

fn screen_side_toward_point(point: (i32, i32), target: (i32, i32)) -> CanvasSide {
    let dx = target.0 - point.0;
    let dy = target.1 - point.1;
    if dx.abs() >= dy.abs() {
        if dx >= 0 {
            CanvasSide::Right
        } else {
            CanvasSide::Left
        }
    } else if dy >= 0 {
        CanvasSide::Bottom
    } else {
        CanvasSide::Top
    }
}

fn border_anchor(rect: ScreenRect, side: CanvasSide) -> (i32, i32) {
    match side {
        CanvasSide::Top => (rect.x + rect.width / 2, rect.y),
        CanvasSide::Right => (rect.right() - 1, rect.y + rect.height / 2),
        CanvasSide::Bottom => (rect.x + rect.width / 2, rect.bottom() - 1),
        CanvasSide::Left => (rect.x, rect.y + rect.height / 2),
    }
}

fn outside_anchor(rect: ScreenRect, side: CanvasSide) -> (i32, i32) {
    let anchor = border_anchor(rect, side);
    match side {
        CanvasSide::Top => (anchor.0, anchor.1 - 1),
        CanvasSide::Right => (anchor.0 + 1, anchor.1),
        CanvasSide::Bottom => (anchor.0, anchor.1 + 1),
        CanvasSide::Left => (anchor.0 - 1, anchor.1),
    }
}

fn path_cells(points: &[(i32, i32)], area: Rect) -> Vec<Position> {
    let mut cells = HashSet::new();
    for segment in points.windows(2) {
        let (from, to) = (segment[0], segment[1]);
        if from.1 == to.1 && from.1 >= area.y as i32 && from.1 < area.bottom() as i32 {
            let start = from.0.min(to.0).max(area.x as i32);
            let end = from.0.max(to.0).min(area.right().saturating_sub(1) as i32);
            for x in start..=end {
                cells.insert(Position::new(x as u16, from.1 as u16));
            }
        } else if from.0 == to.0 && from.0 >= area.x as i32 && from.0 < area.right() as i32 {
            let start = from.1.min(to.1).max(area.y as i32);
            let end = from.1.max(to.1).min(area.bottom().saturating_sub(1) as i32);
            for y in start..=end {
                cells.insert(Position::new(from.0 as u16, y as u16));
            }
        }
    }
    cells.into_iter().collect()
}

fn path_midpoint(points: &[(i32, i32)]) -> (i32, i32) {
    let total = points.windows(2).map(|segment| (segment[1].0 - segment[0].0).abs() + (segment[1].1 - segment[0].1).abs()).sum::<i32>();
    let mut remaining = total / 2;
    for segment in points.windows(2) {
        let length = (segment[1].0 - segment[0].0).abs() + (segment[1].1 - segment[0].1).abs();
        if remaining <= length {
            return if segment[0].0 == segment[1].0 { (segment[0].0, segment[0].1 + (segment[1].1 - segment[0].1).signum() * remaining) } else { (segment[0].0 + (segment[1].0 - segment[0].0).signum() * remaining, segment[0].1) };
        }
        remaining -= length;
    }
    points.last().copied().unwrap_or_default()
}

fn draw_arrow(buffer: &mut Buffer, point: (i32, i32), side: CanvasSide, area: Rect, color: Color, emphasized: bool) {
    let glyph = match side {
        CanvasSide::Top => '▼',
        CanvasSide::Right => '◀',
        CanvasSide::Bottom => '▲',
        CanvasSide::Left => '▶',
    };
    put(buffer, point, glyph, area, Style::default().fg(color).add_modifier(if emphasized { Modifier::BOLD } else { Modifier::empty() }));
}

fn draw_edge_label(buffer: &mut Buffer, label: &str, point: (i32, i32), area: Rect, color: Color, selected: bool, theme: &Theme) {
    let label = truncate_cells(label, 24);
    let width = label.width().saturating_add(2).min(area.width as usize);
    if width == 0 {
        return;
    }
    let x = (point.0 - width as i32 / 2).clamp(area.x as i32, area.right() as i32 - width as i32);
    let y = point.1.saturating_sub(1).clamp(area.y as i32, area.bottom().saturating_sub(1) as i32);
    let rect = Rect::new(x as u16, y as u16, width as u16, 1);
    fill_rect(buffer, rect, theme.background_secondary);
    draw_inline_text(buffer, &format!(" {label} "), (x, y), area, Style::default().fg(color).bg(theme.background_secondary).add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }), width);
}

fn truncate_cells(value: &str, max_width: usize) -> String {
    if value.width() <= max_width {
        return value.to_string();
    }
    let content_width = max_width.saturating_sub(1);
    let mut output = String::new();
    let mut used = 0usize;
    for character in value.chars() {
        let width = character.width().unwrap_or(1);
        if used + width > content_width {
            break;
        }
        output.push(character);
        used += width;
    }
    output.push('…');
    output
}

fn node_type_label(node: &CanvasNode) -> &'static str {
    match node.kind {
        CanvasNodeKind::Text { .. } => "Text",
        CanvasNodeKind::File { .. } => "File",
        CanvasNodeKind::Link { .. } => "Link",
        CanvasNodeKind::Group { .. } => "Group",
        CanvasNodeKind::Unknown { .. } => "Node",
    }
}

fn node_summary(node: &CanvasNode) -> String {
    match &node.kind {
        CanvasNodeKind::Text { text } => text.lines().find(|line| !line.trim().is_empty()).unwrap_or("Text card").trim_start_matches('#').trim().to_string(),
        CanvasNodeKind::File { file, subpath } => subpath.as_ref().map_or_else(|| file.clone(), |subpath| format!("{file} {subpath}")),
        CanvasNodeKind::Link { url } => url.clone(),
        CanvasNodeKind::Group { label, .. } => label.clone().unwrap_or_else(|| "Group".to_string()),
        CanvasNodeKind::Unknown { kind } => format!("Unsupported {kind} node"),
    }
}

fn canvas_color(color: &CanvasColor, app: &App) -> Color {
    match color {
        CanvasColor::Preset(1) => app.state.theme.error,
        CanvasColor::Preset(2) => app.state.theme.warning,
        CanvasColor::Preset(3) => app.state.theme.secondary,
        CanvasColor::Preset(4) => app.state.theme.success,
        CanvasColor::Preset(5) => app.state.theme.info,
        CanvasColor::Preset(6) => app.state.theme.primary,
        CanvasColor::Hex(value) => parse_hex(value).unwrap_or(app.state.theme.info),
        CanvasColor::Preset(_) | CanvasColor::Custom(_) => app.state.theme.info,
    }
}

fn parse_hex(value: &str) -> Option<Color> {
    let value = value.strip_prefix('#')?;
    (value.len() == 6).then(|| {
        let red = u8::from_str_radix(&value[0..2], 16).ok()?;
        let green = u8::from_str_radix(&value[2..4], 16).ok()?;
        let blue = u8::from_str_radix(&value[4..6], 16).ok()?;
        Some(Color::Rgb(red, green, blue))
    })?
}

fn edge_glyph(directions: u8) -> char {
    match directions {
        mask if mask == EDGE_EAST | EDGE_WEST => '─',
        mask if mask == EDGE_NORTH | EDGE_SOUTH => '│',
        mask if mask == EDGE_EAST | EDGE_SOUTH => '╭',
        mask if mask == EDGE_SOUTH | EDGE_WEST => '╮',
        mask if mask == EDGE_NORTH | EDGE_EAST => '╰',
        mask if mask == EDGE_NORTH | EDGE_WEST => '╯',
        mask if mask == EDGE_NORTH | EDGE_EAST | EDGE_SOUTH => '├',
        mask if mask == EDGE_EAST | EDGE_SOUTH | EDGE_WEST => '┬',
        mask if mask == EDGE_NORTH | EDGE_SOUTH | EDGE_WEST => '┤',
        mask if mask == EDGE_NORTH | EDGE_EAST | EDGE_WEST => '┴',
        mask if mask == EDGE_NORTH | EDGE_EAST | EDGE_SOUTH | EDGE_WEST => '┼',
        mask if mask & (EDGE_EAST | EDGE_WEST) != 0 && mask & (EDGE_NORTH | EDGE_SOUTH) != 0 => '┼',
        mask if mask & (EDGE_EAST | EDGE_WEST) != 0 => '─',
        _ => '│',
    }
}

fn fill_rect(buffer: &mut Buffer, area: Rect, background: Color) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_char(' ');
                cell.set_style(Style::default().bg(background));
            }
        }
    }
}

fn draw_inline_text(buffer: &mut Buffer, text: &str, origin: (i32, i32), area: Rect, style: Style, max_width: usize) {
    let mut x = origin.0;
    let mut used = 0usize;
    for character in text.chars() {
        let width = character.width().unwrap_or(1);
        if used + width > max_width {
            break;
        }
        put(buffer, (x, origin.1), character, area, style);
        for offset in 1..width {
            put(buffer, (x + offset as i32, origin.1), ' ', area, style);
        }
        x += width as i32;
        used += width;
    }
}

fn put(buffer: &mut Buffer, point: (i32, i32), character: char, area: Rect, style: Style) {
    if !contains(area, point) {
        return;
    }
    if let Some(cell) = buffer.cell_mut((point.0 as u16, point.1 as u16)) {
        cell.set_char(character);
        cell.set_style(style);
    }
}

fn contains(area: Rect, point: (i32, i32)) -> bool {
    point.0 >= area.x as i32 && point.0 < area.right() as i32 && point.1 >= area.y as i32 && point.1 < area.bottom() as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppDependencies, DialogState};
    use crate::config::Config;
    use ratatui::{backend::TestBackend, Terminal};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_CANVAS_ROOT: AtomicU64 = AtomicU64::new(0);

    struct CanvasFixture {
        app: App,
        root: PathBuf,
    }

    impl CanvasFixture {
        fn new() -> Self {
            let id = NEXT_CANVAS_ROOT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("ekphos-canvas-ui-{}-{id}", std::process::id()));
            let vault = root.join("vault");
            fs::create_dir_all(&vault).unwrap();
            fs::write(vault.join("Aurora.md"), "# Aurora").unwrap();
            for index in 0..12 {
                fs::write(vault.join(format!("Note-{index:02}.md")), format!("# Note {index:02}")).unwrap();
            }
            fs::write(
                vault.join("Board.canvas"),
                r##"{
                  "nodes": [
                    {"id":"group","type":"group","label":"Projects","x":-40,"y":-60,"width":920,"height":420,"color":"6"},
                    {"id":"intro","type":"text","text":"# Plan\nMove cards and connect them.","x":0,"y":0,"width":280,"height":140,"color":"5"},
                    {"id":"aurora","type":"file","file":"Aurora.md","x":380,"y":0,"width":240,"height":120,"color":"4"},
                    {"id":"link","type":"link","url":"https://example.test/issue/89","x":380,"y":220,"width":300,"height":120,"color":"3"}
                  ],
                  "edges": [
                    {"id":"one","fromNode":"intro","fromSide":"right","toNode":"aurora","toSide":"left","toEnd":"arrow","label":"build"},
                    {"id":"two","fromNode":"aurora","fromSide":"bottom","toNode":"link","toSide":"top","toEnd":"arrow"}
                  ]
                }"##,
            )
            .unwrap();
            let config = Config { general: crate::config::GeneralConfig { welcome_shown: false, check_updates: false, ..Default::default() }, ..Default::default() };
            let dependencies = AppDependencies::headless(root.join("config"), root.join("cache"));
            let mut app = App::new_injected(config, vault.clone(), None, dependencies);
            app.state.show_welcome = false;
            app.state.dialog = DialogState::None;
            app.state.focus = Focus::Content;
            assert!(app.select_note_by_path(&vault.join("Board.canvas")));
            Self { app, root }
        }

        fn draw(&mut self, width: u16, height: u16) -> Buffer {
            self.app.structured.canvas.needs_fit = true;
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    render_canvas_view(frame, &mut self.app, area);
                })
                .unwrap();
            terminal.backend().buffer().clone()
        }
    }

    impl Drop for CanvasFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn orthogonal_edges_use_connected_corner_glyphs() {
        let area = Rect::new(0, 0, 20, 8);
        let mut buffer = Buffer::empty(area);
        let mut layer = EdgeLayer::new(area);
        layer.add_path(&[(1, 1), (6, 1), (6, 5), (12, 5)], Color::Blue, false);
        layer.render(&mut buffer);

        assert_eq!(buffer.cell((6, 1)).unwrap().symbol(), "╮");
        assert_eq!(buffer.cell((6, 5)).unwrap().symbol(), "╰");
        assert_eq!(buffer.cell((9, 5)).unwrap().symbol(), "─");
    }

    #[test]
    fn cards_are_opaque_over_edge_paths_and_use_one_surface() {
        let area = Rect::new(0, 0, 24, 10);
        let mut buffer = Buffer::empty(area);
        for x in 0..24 {
            buffer.cell_mut((x, 5)).unwrap().set_char('─').set_bg(Color::Red);
        }
        let node = CanvasNode { id: "card".to_string(), x: 0, y: 0, width: 200, height: 100, color: None, kind: CanvasNodeKind::Text { text: "Hello".to_string() }, extra: Default::default() };
        let theme = Theme::default();
        draw_card(&mut buffer, &node, ScreenRect { x: 5, y: 2, width: 14, height: 6 }, area, NodeAppearance { accent: theme.info, selected: false, hovered: false }, None, &theme);

        for x in 5..19 {
            assert_ne!(buffer.cell((x, 5)).unwrap().symbol(), "─", "edge leaked through at x={x}");
            assert_eq!(buffer.cell((x, 5)).unwrap().bg, theme.background_secondary);
        }
    }

    #[test]
    fn card_compositing_never_paints_outside_the_canvas_viewport() {
        let buffer_area = Rect::new(0, 0, 20, 10);
        let graph_area = Rect::new(3, 2, 12, 6);
        let mut buffer = Buffer::empty(buffer_area);
        buffer.cell_mut((2, 3)).unwrap().set_char('X');
        let node = CanvasNode { id: "clipped".to_string(), x: 0, y: 0, width: 200, height: 100, color: None, kind: CanvasNodeKind::Text { text: "Clipped".to_string() }, extra: Default::default() };
        let theme = Theme::default();
        draw_card(&mut buffer, &node, ScreenRect { x: 0, y: 1, width: 10, height: 6 }, graph_area, NodeAppearance { accent: theme.info, selected: true, hovered: false }, None, &theme);

        assert_eq!(buffer.cell((2, 3)).unwrap().symbol(), "X");
        assert_eq!(buffer.cell((3, 3)).unwrap().bg, theme.selection);
    }

    #[test]
    fn complete_canvas_reflows_without_transparent_card_cells() {
        for (width, height) in [(28, 9), (60, 18), (100, 30), (160, 50)] {
            let mut fixture = CanvasFixture::new();
            let buffer = fixture.draw(width, height);
            if width < 40 {
                continue;
            }
            let mut symbols = String::new();
            for y in 0..buffer.area.height {
                for x in 0..buffer.area.width {
                    symbols.push_str(buffer.cell((x, y)).unwrap().symbol());
                }
            }
            if width >= 60 {
                assert!(symbols.contains("shortcuts"), "shortcut toggle was clipped at {width}x{height}");
            }
            assert!(!fixture.app.structured.canvas.node_rects.is_empty());
            assert_eq!(fixture.app.structured.canvas.handle_rects.len(), 4);
            for (_, rect) in &fixture.app.structured.canvas.node_rects {
                for y in rect.y..rect.bottom() {
                    for x in rect.x..rect.right() {
                        assert_ne!(buffer.cell((x, y)).unwrap().bg, Color::Reset, "transparent card cell at {x},{y} in {width}x{height}");
                    }
                }
            }
        }
    }

    #[test]
    fn canvas_overlays_and_expanded_footer_survive_tiny_viewports() {
        for (width, height) in [(1, 1), (2, 2), (8, 3), (16, 5)] {
            let mut fixture = CanvasFixture::new();
            fixture.app.structured.canvas.shortcuts_expanded = true;
            fixture.app.structured.canvas.overlay = CanvasOverlay::Menu(crate::app::CanvasMenuState {
                screen_position: Position::new(0, 0),
                world_position: (0, 0),
                target: CanvasMenuTarget::Background,
                items: vec![CanvasMenuAction::AddText, CanvasMenuAction::AddFile],
                selected_index: 0,
                area: Rect::default(),
                item_rects: Vec::new(),
            });
            fixture.draw(width, height);
            assert!(fixture.app.structured.canvas.view_area.height <= height);

            fixture.app.structured.canvas.overlay = CanvasOverlay::FilePicker(crate::app::CanvasFilePickerState { world_position: (0, 0), query: String::new(), results: Vec::new(), selected_index: 0, scroll_offset: 0, area: Rect::default(), result_rects: Vec::new(), last_click: None });
            fixture.draw(width, height);
        }
    }

    #[test]
    fn canvas_footer_starts_compact_and_expands_on_demand() {
        let mut fixture = CanvasFixture::new();
        let compact = fixture.draw(100, 30);
        let compact_symbols = compact.content.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(compact_symbols.contains("shortcuts"));
        assert!(!compact_symbols.contains("Shift+arrows"));
        assert!(fixture.app.structured.canvas.shortcut_toggle_rect.is_some());

        fixture.app.canvas_toggle_shortcuts();
        let expanded = fixture.draw(100, 30);
        let expanded_symbols = expanded.content.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(expanded_symbols.contains("hide"));
        assert!(expanded_symbols.contains("Shift+arrows"));
        assert!(expanded_symbols.contains("Ctrl+r"));
        assert!(expanded_symbols.contains("source"));
    }

    #[test]
    fn canvas_editor_footer_matches_save_and_newline_controls() {
        let mut fixture = CanvasFixture::new();
        fixture.draw(100, 30);
        fixture.app.canvas_select_node(1);
        assert!(fixture.app.canvas_begin_node_edit());
        assert_eq!(fixture.app.state.status_message.as_deref(), Some("Editing card · Ctrl+S saves · Esc cancels"));

        let buffer = fixture.draw(100, 30);
        let symbols = buffer.content.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(symbols.contains("Ctrl+S save"));
        assert!(symbols.contains("Enter new line"));
        assert!(!symbols.contains("Ctrl+Enter"));
    }

    #[test]
    fn inline_editor_stays_inside_the_selected_card_and_hides_drag_handles() {
        let mut fixture = CanvasFixture::new();
        assert!(fixture.app.canvas_begin_node_edit());
        assert!(fixture.app.canvas_edit_insert("\nDirect edit"));

        let buffer = fixture.draw(100, 30);
        let mut symbols = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                symbols.push_str(buffer.cell((x, y)).unwrap().symbol());
            }
        }

        assert!(symbols.contains("Editing text"));
        assert!(!symbols.contains('▏'));
        assert!(buffer.content.iter().any(|cell| cell.modifier.contains(Modifier::REVERSED)));
        assert!(fixture.app.structured.canvas.handle_rects.is_empty());
        assert_eq!(fixture.app.structured.canvas.resize_rects.len(), 8);
        assert!(fixture.app.structured.canvas.editor.is_some());
    }

    #[test]
    fn canvas_context_menu_and_file_picker_render_mouse_hit_targets() {
        let mut fixture = CanvasFixture::new();
        fixture.draw(100, 30);
        let view_area = fixture.app.structured.canvas.view_area;
        let pointer = Position::new(view_area.x + view_area.width / 2, view_area.y + view_area.height / 2);
        assert!(fixture.app.canvas_open_context_menu(pointer, CanvasMenuTarget::Background));
        if let CanvasOverlay::Menu(menu) = &mut fixture.app.structured.canvas.overlay {
            menu.selected_index = menu.items.len().saturating_sub(1);
        }

        let menu_buffer = fixture.draw(100, 30);
        let symbols = menu_buffer.content.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(symbols.contains("Add text card"));
        assert!(symbols.contains("Add file card"));
        let CanvasOverlay::Menu(menu) = &fixture.app.structured.canvas.overlay else {
            panic!("Canvas menu should remain open");
        };
        assert_eq!(menu.item_rects.len(), menu.items.len());
        assert!(menu.item_rects.iter().all(|(_, rect)| rect.width > 0));
        let selected_row = menu.item_rects[menu.selected_index].1;
        assert!((selected_row.x..selected_row.right()).all(|x| menu_buffer.cell((x, selected_row.y)).is_some_and(|cell| cell.bg == fixture.app.state.theme.primary)));
        assert_eq!(menu_buffer.cell((selected_row.right() - 1, selected_row.y)).map(|cell| cell.symbol()), Some(" "));

        assert!(fixture.app.canvas_execute_menu_action(CanvasMenuAction::AddFile));
        let picker_buffer = fixture.draw(100, 30);
        let symbols = picker_buffer.content.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(symbols.contains("Add file card"));
        assert!(symbols.contains("Search:"));
        let CanvasOverlay::FilePicker(picker) = &fixture.app.structured.canvas.overlay else {
            panic!("Canvas file picker should remain open");
        };
        assert!(!picker.result_rects.is_empty());
        let selected_row = picker.result_rects.iter().find(|(index, _)| *index == picker.selected_index).map(|(_, rect)| *rect).expect("selected file row should be visible");
        assert!((selected_row.x..selected_row.right()).all(|x| picker_buffer.cell((x, selected_row.y)).is_some_and(|cell| cell.bg == fixture.app.state.theme.primary)));
    }

    #[test]
    fn canvas_file_picker_keeps_keyboard_selection_visible_in_short_views() {
        let mut fixture = CanvasFixture::new();
        fixture.draw(100, 12);
        let view_area = fixture.app.structured.canvas.view_area;
        assert!(fixture.app.canvas_open_context_menu(Position::new(view_area.x, view_area.y), CanvasMenuTarget::Background));
        assert!(fixture.app.canvas_execute_menu_action(CanvasMenuAction::AddFile));
        fixture.draw(100, 12);

        let initial_height = match &fixture.app.structured.canvas.overlay {
            CanvasOverlay::FilePicker(picker) => picker.area.height,
            _ => panic!("file picker should be open"),
        };
        assert!(fixture.app.canvas_file_picker_move_page(1));
        fixture.draw(100, 12);

        let CanvasOverlay::FilePicker(picker) = &fixture.app.structured.canvas.overlay else {
            panic!("file picker should remain open");
        };
        assert_eq!(picker.area.height, initial_height);
        assert!(picker.result_rects.iter().any(|(index, _)| *index == picker.selected_index));
    }

    #[test]
    fn overflowing_editor_keeps_the_caret_visible_and_discloses_hidden_rows() {
        let area = Rect::new(0, 0, 22, 8);
        let mut buffer = Buffer::empty(area);
        let node = CanvasNode { id: "card".to_string(), x: 0, y: 0, width: 200, height: 100, color: None, kind: CanvasNodeKind::Text { text: String::new() }, extra: Default::default() };
        let theme = Theme::default();
        let mut editor = CanvasNodeEditor::new(0, crate::app::CanvasNodeEditField::Text, "one\ntwo\nthree\nfour\nfive".to_string());

        let caret = draw_card(&mut buffer, &node, ScreenRect { x: 2, y: 1, width: 18, height: 5 }, area, NodeAppearance { accent: theme.info, selected: true, hovered: false }, Some(&mut editor), &theme);

        assert!(caret.is_some());
        assert!(editor.scroll_row > 0);
        assert!(editor.total_rows > editor.viewport_height);
        let symbols = buffer.content.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(symbols.contains('▲'));
        assert!(!symbols.contains('▼'));
        assert!(buffer.content.iter().any(|cell| cell.modifier.contains(Modifier::REVERSED)));
    }
}
