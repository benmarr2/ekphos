use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::App;
use crate::config::{EditingMode, Theme};
use crate::keybindings::{AppCommand, KeybindingFallback};

const TITLE_MAIN: &[&str] = &["████████ ██   ██ ██████  ██   ██  ██████  ███████", "██       ██  ██  ██   ██ ██   ██ ██    ██ ██     ", "█████    █████   ██████  ███████ ██    ██ ███████", "██       ██  ██  ██      ██   ██ ██    ██      ██", "████████ ██   ██ ██      ██   ██  ██████  ███████"];

struct HelpEntry {
    commands: &'static [AppCommand],
    description: &'static str,
}

struct HelpSection {
    title: &'static str,
    entries: &'static [HelpEntry],
}

const HELP_SECTIONS: &[HelpSection] = &[
    HelpSection {
        title: "Global",
        entries: &[
            HelpEntry { commands: &[AppCommand::ShowHelp], description: "Show help" },
            HelpEntry { commands: &[AppCommand::Quit], description: "Quit" },
            HelpEntry { commands: &[AppCommand::FocusNext], description: "Focus next panel" },
            HelpEntry { commands: &[AppCommand::FocusPrevious], description: "Focus previous panel" },
            HelpEntry { commands: &[AppCommand::ToggleSidebar], description: "Toggle sidebar" },
            HelpEntry { commands: &[AppCommand::ToggleOutline], description: "Toggle outline" },
            HelpEntry { commands: &[AppCommand::ShrinkPanel, AppCommand::GrowPanel], description: "Shrink / grow panel" },
            HelpEntry { commands: &[AppCommand::OpenQuickSearch], description: "Search notes" },
            HelpEntry { commands: &[AppCommand::FindInBuffer], description: "Find in note" },
            HelpEntry { commands: &[AppCommand::OpenThemeSelector], description: "Select theme and style" },
            HelpEntry { commands: &[AppCommand::OpenGraph], description: "Open graph view" },
            HelpEntry { commands: &[AppCommand::OpenTaskView], description: "Open task view" },
            HelpEntry { commands: &[AppCommand::OpenJournal], description: "Open today's journal" },
            HelpEntry { commands: &[AppCommand::HistoryBack, AppCommand::HistoryForward], description: "Go back / forward" },
            HelpEntry { commands: &[AppCommand::ToggleZen], description: "Toggle zen mode" },
            HelpEntry { commands: &[AppCommand::ReloadFiles], description: "Reload files from disk" },
            HelpEntry { commands: &[AppCommand::ReloadConfig], description: "Reload config and theme" },
        ],
    },
    HelpSection {
        title: "Navigation",
        entries: &[
            HelpEntry { commands: &[AppCommand::MoveDown, AppCommand::MoveUp], description: "Move down / up" },
            HelpEntry { commands: &[AppCommand::GoFirst, AppCommand::GoLast], description: "Go to first / last" },
            HelpEntry { commands: &[AppCommand::Activate], description: "Activate selected item" },
            HelpEntry { commands: &[AppCommand::OpenSelected], description: "Open selected target" },
        ],
    },
    HelpSection {
        title: "Sidebar",
        entries: &[
            HelpEntry { commands: &[AppCommand::CreateNote], description: "Create note" },
            HelpEntry { commands: &[AppCommand::CreateFolder], description: "Create folder" },
            HelpEntry { commands: &[AppCommand::EditNote], description: "Edit note" },
            HelpEntry { commands: &[AppCommand::RenameItem], description: "Rename item" },
            HelpEntry { commands: &[AppCommand::DeleteItem], description: "Delete item" },
            HelpEntry { commands: &[AppCommand::CutItem], description: "Cut item" },
            HelpEntry { commands: &[AppCommand::PasteItem], description: "Paste item" },
            HelpEntry { commands: &[AppCommand::CancelCut], description: "Cancel cut" },
            HelpEntry { commands: &[AppCommand::SidebarSearch], description: "Search sidebar" },
            HelpEntry { commands: &[AppCommand::CycleSort], description: "Change sort order" },
        ],
    },
    HelpSection {
        title: "Content view",
        entries: &[
            HelpEntry { commands: &[AppCommand::ContentAction], description: "Toggle task / open target" },
            HelpEntry { commands: &[AppCommand::NextTarget, AppCommand::PreviousTarget], description: "Next / previous target" },
            HelpEntry { commands: &[AppCommand::ToggleFloatingCursor], description: "Toggle floating cursor" },
            HelpEntry { commands: &[AppCommand::HalfPageDown, AppCommand::HalfPageUp], description: "Half-page down / up" },
            HelpEntry { commands: &[AppCommand::ToggleFrontmatter], description: "Toggle frontmatter" },
            HelpEntry { commands: &[AppCommand::ToggleFold], description: "Toggle heading fold" },
            HelpEntry { commands: &[AppCommand::FoldAll, AppCommand::UnfoldAll], description: "Fold / unfold all headings" },
        ],
    },
    HelpSection {
        title: "Editor",
        entries: &[HelpEntry { commands: &[AppCommand::ToggleEditorMode], description: "Switch editing mode" }, HelpEntry { commands: &[AppCommand::InsertTask], description: "Insert task item" }, HelpEntry { commands: &[AppCommand::ToggleEditorFold], description: "Toggle heading fold" }],
    },
];

pub fn render_keybinding_warning(f: &mut Frame, app: &App) {
    let Some(warning) = &app.state.keybinding_warning else { return };
    let area = f.area();
    let width = 76.min(area.width.saturating_sub(4));
    let height = 26.min(area.height.saturating_sub(4));
    if width < 20 || height < 8 {
        return;
    }
    let dialog_area = Rect { x: area.x + area.width.saturating_sub(width) / 2, y: area.y + area.height.saturating_sub(height) / 2, width, height };
    f.render_widget(Clear, dialog_area);
    let block = Block::default().title(Span::styled(" Keybinding Warning ", Style::default().fg(app.state.theme.warning).add_modifier(Modifier::BOLD))).borders(Borders::ALL).border_style(Style::default().fg(app.state.theme.warning)).style(Style::default().bg(app.state.theme.background_secondary));
    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);
    let fallback = match warning.fallback {
        KeybindingFallback::Defaults => "Custom keybindings were not applied; built-in defaults are active.",
        KeybindingFallback::Previous => "Custom keybindings were not applied; the previous valid bindings remain active.",
    };
    let sections = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(4)]).split(inner);
    let header = vec![Line::from(Span::styled(fallback, Style::default().fg(app.state.theme.foreground))), Line::from(""), Line::from(Span::styled("Resolve every item below:", Style::default().fg(app.state.theme.error).add_modifier(Modifier::BOLD)))];
    f.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);
    let issues: Vec<Line> = warning.issues.iter().map(|issue| Line::from(vec![Span::styled("• ", Style::default().fg(app.state.theme.error)), Span::styled(issue, Style::default().fg(app.state.theme.foreground))])).collect();
    f.render_widget(Paragraph::new(issues).wrap(Wrap { trim: false }).scroll((warning.scroll.min(u16::MAX as usize) as u16, 0)), sections[1]);
    let footer = vec![
        Line::from(Span::styled("Use j/k, arrows, or Page Up/Down to scroll.", Style::default().fg(app.state.theme.muted))),
        Line::from(vec![Span::styled("Config: ", Style::default().fg(app.state.theme.muted)), Span::styled(app.config_path().display().to_string(), Style::default().fg(app.state.theme.info))]),
        Line::from(Span::styled("Edit the config and reload. Press Enter or Esc to dismiss.", Style::default().fg(app.state.theme.muted))),
    ];
    f.render_widget(Paragraph::new(footer).alignment(Alignment::Left).wrap(Wrap { trim: false }), sections[2]);
}
fn render_flat_title(theme: &Theme, dialog_width: u16) -> Vec<Line<'static>> {
    let main_color = theme.dialog.title;
    let title_width = TITLE_MAIN[0].chars().count();
    let inner_width = dialog_width.saturating_sub(2) as usize; // minus borders
    let left_pad = inner_width.saturating_sub(title_width) / 2;
    let padding = " ".repeat(left_pad);
    TITLE_MAIN
        .iter()
        .map(|line| {
            let styled_line: String = line.chars().map(|ch| if ch != ' ' { '█' } else { ' ' }).collect();
            Line::from(vec![Span::raw(padding.clone()), Span::styled(styled_line, Style::default().fg(main_color))]).alignment(Alignment::Left)
        })
        .collect()
}

pub fn render_welcome_dialog(f: &mut Frame, theme: &Theme) {
    let area = f.area();
    let dialog_theme = &theme.dialog;
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 24.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let mut welcome_text = vec![Line::from("")];
    welcome_text.extend(render_flat_title(theme, dialog_width));
    welcome_text.extend(vec![
        Line::from(""),
        Line::from(Span::styled("A lightweight markdown research tool", Style::default().fg(dialog_theme.text))),
        Line::from(""),
        Line::from(vec![Span::styled("j/k ", Style::default().fg(theme.warning)), Span::styled("Navigate notes", Style::default().fg(dialog_theme.text))]),
        Line::from(vec![Span::styled("Tab ", Style::default().fg(theme.warning)), Span::styled("Switch focus  ", Style::default().fg(dialog_theme.text))]),
        Line::from(vec![Span::styled("e   ", Style::default().fg(theme.warning)), Span::styled("Edit note     ", Style::default().fg(dialog_theme.text))]),
        Line::from(vec![Span::styled("?   ", Style::default().fg(theme.warning)), Span::styled("Help          ", Style::default().fg(dialog_theme.text))]),
        Line::from(vec![Span::styled("q   ", Style::default().fg(theme.warning)), Span::styled("Quit          ", Style::default().fg(dialog_theme.text))]),
        Line::from(""),
        Line::from(Span::styled("Read the docs at docs.ekphos.xyz", Style::default().fg(dialog_theme.text))),
        Line::from(""),
        Line::from(Span::styled("Press Enter or Space to continue", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))),
    ]);
    let welcome = Paragraph::new(welcome_text).block(Block::default().title(" Welcome ").borders(Borders::ALL).border_style(Style::default().fg(dialog_theme.border)).style(Style::default().bg(dialog_theme.background))).alignment(Alignment::Center);
    f.render_widget(welcome, dialog_area);
}

pub fn render_onboarding_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.state.theme;
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 12.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let content = vec![
        Line::from(""),
        Line::from(Span::styled("Welcome to Ekphos!", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled("Where would you like to store your notes?", Style::default().fg(theme.foreground))),
        Line::from(""),
        Line::from(vec![Span::styled("> ", Style::default().fg(theme.warning)), Span::styled(&app.state.input_buffer, Style::default().fg(theme.foreground)), Span::styled("█", Style::default().fg(theme.cursor))]),
        Line::from(""),
        Line::from(Span::styled("Press Enter to confirm", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))),
    ];
    let dialog = Paragraph::new(content).block(Block::default().title(" Setup ").borders(Borders::ALL).border_style(Style::default().fg(theme.primary)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
    f.render_widget(dialog, dialog_area);
}

pub fn render_create_note_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.state.theme;
    let has_context = app.vault.target_folder.is_some();
    let has_error = app.state.dialog_error.is_some();
    let base_height = if has_context { 10 } else { 9 };
    let dialog_height = if has_error { base_height + 2 } else { base_height };
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = dialog_height.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let mut content = vec![Line::from(""), Line::from(Span::styled("Enter note name:", Style::default().fg(theme.foreground)))];
    if let Some(ref folder_path) = app.vault.target_folder {
        if let Some(folder_name) = folder_path.file_name() {
            content.push(Line::from(Span::styled(format!("in {}/", folder_name.to_string_lossy()), Style::default().fg(theme.info))));
        }
    }
    content.push(Line::from(""));
    content.push(Line::from(vec![Span::styled("> ", Style::default().fg(theme.warning)), Span::styled(&app.state.input_buffer, Style::default().fg(theme.foreground)), Span::styled("█", Style::default().fg(theme.cursor))]));
    if let Some(ref error) = app.state.dialog_error {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(error.as_str(), Style::default().fg(theme.error))));
    }
    content.push(Line::from(""));
    content.push(Line::from(Span::styled("Enter: Create  |  Esc: Cancel", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))));
    let border_color = if has_error { theme.error } else { theme.success };
    let dialog = Paragraph::new(content).block(Block::default().title(" New Note ").borders(Borders::ALL).border_style(Style::default().fg(border_color)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
    f.render_widget(dialog, dialog_area);
}

pub fn render_delete_confirm_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.state.theme;
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 9.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let note_name = app.current_note().map(|n| n.title.as_str()).unwrap_or("this note");
    let content = vec![
        Line::from(""),
        Line::from(Span::styled("Delete note?", Style::default().fg(theme.error).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled(note_name, Style::default().fg(theme.foreground))),
        Line::from(""),
        Line::from(Span::styled("y: Yes  |  n: No", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))),
    ];
    let dialog = Paragraph::new(content).block(Block::default().title(" Confirm Delete ").borders(Borders::ALL).border_style(Style::default().fg(theme.error)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
    f.render_widget(dialog, dialog_area);
}

pub fn render_unsaved_changes_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.state.theme;
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 10.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let content = vec![
        Line::from(""),
        Line::from(Span::styled("Save changes before returning to preview?", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled("Choose an action for your changes.", Style::default().fg(theme.foreground))),
        Line::from(""),
        Line::from(Span::styled("S: Save  |  D: Discard  |  Esc: Keep editing", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))),
    ];
    let dialog = Paragraph::new(content).block(Block::default().title(" Unsaved Changes ").borders(Borders::ALL).border_style(Style::default().fg(theme.warning)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
    f.render_widget(dialog, dialog_area);
}

pub fn render_create_wiki_note_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.state.theme;
    let dialog_width = 55.min(area.width.saturating_sub(4));
    let dialog_height = 10.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let target = app.editor.pending_wiki_target.as_deref().unwrap_or("note");
    let content = vec![
        Line::from(""),
        Line::from(Span::styled(format!("Note '[[{}]]' doesn't exist.", target), Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled("Would you like to create it?", Style::default().fg(theme.foreground))),
        Line::from(""),
        Line::from(Span::styled("y: Create  |  n: Cancel", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))),
    ];
    let dialog = Paragraph::new(content).block(Block::default().title(" Create Note ").borders(Borders::ALL).border_style(Style::default().fg(theme.info)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
    f.render_widget(dialog, dialog_area);
}

pub fn render_delete_folder_confirm_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.state.theme;
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 11.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let folder_name = app.get_selected_folder_name().unwrap_or_else(|| "this folder".to_string());
    let content = vec![
        Line::from(""),
        Line::from(Span::styled("Delete folder and all contents?", Style::default().fg(theme.error).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled(folder_name, Style::default().fg(theme.foreground))),
        Line::from(""),
        Line::from(Span::styled("This will delete all notes inside!", Style::default().fg(theme.warning))),
        Line::from(""),
        Line::from(Span::styled("y: Yes  |  n: No", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))),
    ];
    let dialog = Paragraph::new(content).block(Block::default().title(" Confirm Delete Folder ").borders(Borders::ALL).border_style(Style::default().fg(theme.error)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
    f.render_widget(dialog, dialog_area);
}

pub fn render_rename_note_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.state.theme;
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 9.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let content = vec![
        Line::from(""),
        Line::from(Span::styled("Enter new name:", Style::default().fg(theme.foreground))),
        Line::from(""),
        Line::from(vec![Span::styled("> ", Style::default().fg(theme.warning)), Span::styled(&app.state.input_buffer, Style::default().fg(theme.foreground)), Span::styled("█", Style::default().fg(theme.cursor))]),
        Line::from(""),
        Line::from(Span::styled("Enter: Rename  |  Esc: Cancel", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))),
    ];
    let dialog = Paragraph::new(content).block(Block::default().title(" Rename Note ").borders(Borders::ALL).border_style(Style::default().fg(theme.warning)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
    f.render_widget(dialog, dialog_area);
}

pub fn render_rename_folder_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.state.theme;
    let has_error = app.state.dialog_error.is_some();
    let dialog_height = if has_error { 11 } else { 9 };
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = dialog_height.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let mut content = vec![
        Line::from(""),
        Line::from(Span::styled("Enter new folder name:", Style::default().fg(theme.foreground))),
        Line::from(""),
        Line::from(vec![Span::styled("> ", Style::default().fg(theme.info)), Span::styled(&app.state.input_buffer, Style::default().fg(theme.foreground)), Span::styled("█", Style::default().fg(theme.cursor))]),
    ];
    if let Some(ref error) = app.state.dialog_error {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(error.as_str(), Style::default().fg(theme.error))));
    }
    content.push(Line::from(""));
    content.push(Line::from(Span::styled("Enter: Rename  |  Esc: Cancel", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))));
    let border_color = if has_error { theme.error } else { theme.info };
    let dialog = Paragraph::new(content).block(Block::default().title(" Rename Folder ").borders(Borders::ALL).border_style(Style::default().fg(border_color)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
    f.render_widget(dialog, dialog_area);
}

pub fn render_create_folder_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.state.theme;
    let has_error = app.state.dialog_error.is_some();
    let has_context = app.vault.target_folder.is_some();
    let dialog_height = if has_error {
        11
    } else if has_context {
        10
    } else {
        9
    };
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = dialog_height.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let mut content = vec![Line::from(""), Line::from(Span::styled("Enter folder name:", Style::default().fg(theme.foreground)))];
    if let Some(ref folder_path) = app.vault.target_folder {
        if let Some(folder_name) = folder_path.file_name() {
            content.push(Line::from(Span::styled(format!("in {}/", folder_name.to_string_lossy()), Style::default().fg(theme.info))));
        }
    }
    content.push(Line::from(""));
    content.push(Line::from(vec![Span::styled("> ", Style::default().fg(theme.info)), Span::styled(&app.state.input_buffer, Style::default().fg(theme.foreground)), Span::styled("█", Style::default().fg(theme.cursor))]));
    if let Some(ref error) = app.state.dialog_error {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(error.as_str(), Style::default().fg(theme.error))));
    }
    content.push(Line::from(""));
    content.push(Line::from(Span::styled("Enter: Create  |  Esc: Cancel", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))));
    let border_color = if has_error { theme.error } else { theme.info };
    let dialog = Paragraph::new(content).block(Block::default().title(" New Folder ").borders(Borders::ALL).border_style(Style::default().fg(border_color)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
    f.render_widget(dialog, dialog_area);
}

pub fn render_create_note_in_folder_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.state.theme;
    let has_error = app.state.dialog_error.is_some();
    let dialog_height = if has_error { 13 } else { 11 };
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = dialog_height.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let folder_name = app.vault.target_folder.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "folder".to_string());
    let mut content = vec![
        Line::from(""),
        Line::from(Span::styled("Folder created! Now create your first note:", Style::default().fg(theme.success))),
        Line::from(Span::styled(format!("in {}/", folder_name), Style::default().fg(theme.info))),
        Line::from(""),
        Line::from(vec![Span::styled("> ", Style::default().fg(theme.warning)), Span::styled(&app.state.input_buffer, Style::default().fg(theme.foreground)), Span::styled("█", Style::default().fg(theme.cursor))]),
    ];
    if let Some(ref error) = app.state.dialog_error {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(error.as_str(), Style::default().fg(theme.error))));
    }
    content.push(Line::from(""));
    content.push(Line::from(Span::styled("Enter: Create  |  Esc: Cancel (removes folder)", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))));
    let border_color = if has_error { theme.error } else { theme.success };
    let dialog = Paragraph::new(content).block(Block::default().title(" New Note in Folder ").borders(Borders::ALL).border_style(Style::default().fg(border_color)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
    f.render_widget(dialog, dialog_area);
}

pub fn render_empty_directory_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.state.theme;
    let dialog_width = 55.min(area.width.saturating_sub(4));
    let dialog_height = 12.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let content = vec![
        Line::from(""),
        Line::from(Span::styled("Oops! This directory seems empty", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled("No markdown notes found in:", Style::default().fg(theme.foreground))),
        Line::from(Span::styled(&app.state.config.notes_dir, Style::default().fg(theme.muted))),
        Line::from(""),
        Line::from(Span::styled(format!("Press {} to create your first note!", app.state.keymap.binding_label(AppCommand::CreateNote)), Style::default().fg(theme.success))),
        Line::from(""),
        Line::from(Span::styled("Press Enter or Esc to continue", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))),
    ];
    let dialog = Paragraph::new(content).block(Block::default().title(" Getting Started ").borders(Borders::ALL).border_style(Style::default().fg(theme.warning)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
    f.render_widget(dialog, dialog_area);
}

pub fn render_help_dialog(f: &mut Frame, app: &App) -> usize {
    let area = f.area();
    let theme = &app.state.theme;
    let dialog_theme = &theme.dialog;
    let dialog_width = 90.min(area.width.saturating_sub(4));
    let dialog_height = area.height.saturating_sub(4).min(50);
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let block = Block::default().title(" Help - All keybindings (j/k or arrows to scroll) ").borders(Borders::ALL).border_style(Style::default().fg(dialog_theme.border)).style(Style::default().bg(dialog_theme.background));
    let inner_area = block.inner(dialog_area);
    f.render_widget(block, dialog_area);
    let columns = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner_area);
    let key_style = Style::default().fg(theme.warning);
    let desc_style = Style::default().fg(dialog_theme.text);
    let header_style = Style::default().fg(dialog_theme.title).add_modifier(Modifier::BOLD);
    let subheader_style = Style::default().fg(theme.info).add_modifier(Modifier::BOLD);
    let keys = |command| format!(" {:<16}", app.state.keymap.binding_label(command));
    let mut left_content = vec![Line::from("")];
    for section in HELP_SECTIONS {
        left_content.push(Line::from(Span::styled(format!(" {}", section.title), header_style)));
        for entry in section.entries {
            let binding = entry.commands.iter().map(|command| app.state.keymap.binding_label(*command)).collect::<Vec<_>>().join(" / ");
            left_content.push(Line::from(vec![Span::styled(format!(" {binding:<16} "), key_style), Span::styled(entry.description, desc_style)]));
        }
        left_content.push(Line::from(""));
    }
    left_content.extend([
        Line::from(Span::styled(" Help dialog", header_style)),
        Line::from(vec![Span::styled(format!(" {:<21} ", "j/k or ↑/↓"), key_style), Span::styled("Scroll one line", desc_style)]),
        Line::from(vec![Span::styled(format!(" {:<21} ", "Ctrl+d/u or PgDn/PgUp"), key_style), Span::styled("Scroll one page", desc_style)]),
        Line::from(vec![Span::styled(format!(" {:<21} ", "g/G or Home/End"), key_style), Span::styled("Go to first / last", desc_style)]),
        Line::from(vec![Span::styled(format!(" {:<21} ", "Esc/Enter/q/?"), key_style), Span::styled("Close help", desc_style)]),
    ]);
    let right_content = if app.state.config.editor.mode == EditingMode::Standard {
        vec![
            Line::from(""),
            Line::from(Span::styled(" Standard editing", header_style)),
            Line::from(""),
            Line::from(Span::styled("  Write and move", subheader_style)),
            Line::from(vec![Span::styled(" Type      ", key_style), Span::styled("Insert text", desc_style)]),
            Line::from(vec![Span::styled(" Arrows    ", key_style), Span::styled("Move the cursor", desc_style)]),
            Line::from(vec![Span::styled(" Home/End  ", key_style), Span::styled("Move to line start/end", desc_style)]),
            Line::from(vec![Span::styled(" PgUp/PgDn ", key_style), Span::styled("Move one page", desc_style)]),
            Line::from(vec![Span::styled(keys(AppCommand::ToggleEditorFold), key_style), Span::styled("Fold heading; indent elsewhere", desc_style)]),
            Line::from(vec![Span::styled(" Ctrl+←/→  ", key_style), Span::styled("Move by word", desc_style)]),
            Line::from(vec![Span::styled(" Ctrl+Home ", key_style), Span::styled("Move to document start", desc_style)]),
            Line::from(vec![Span::styled(" Ctrl+End  ", key_style), Span::styled("Move to document end", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Select and edit", subheader_style)),
            Line::from(vec![Span::styled(" Shift+move", key_style), Span::styled("Extend selection", desc_style)]),
            Line::from(vec![Span::styled(" Ctrl+a    ", key_style), Span::styled("Select all", desc_style)]),
            Line::from(vec![Span::styled(" Ctrl+c/x  ", key_style), Span::styled("Copy / Cut", desc_style)]),
            Line::from(vec![Span::styled(" Ctrl+v    ", key_style), Span::styled("Paste", desc_style)]),
            Line::from(vec![Span::styled(" Ctrl+z    ", key_style), Span::styled("Undo", desc_style)]),
            Line::from(vec![Span::styled(" Ctrl+y    ", key_style), Span::styled("Redo", desc_style)]),
            Line::from(vec![Span::styled(keys(AppCommand::InsertTask), key_style), Span::styled("Insert task item", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Find, save, and preview", subheader_style)),
            Line::from(vec![Span::styled(" Ctrl+f/w  ", key_style), Span::styled("Find in note", desc_style)]),
            Line::from(vec![Span::styled(" Ctrl+s/o  ", key_style), Span::styled("Save and keep editing", desc_style)]),
            Line::from(vec![Span::styled(" Esc       ", key_style), Span::styled("Return to preview", desc_style)]),
            Line::from(vec![Span::styled(" F1        ", key_style), Span::styled("Show this help", desc_style)]),
            Line::from(vec![Span::styled(keys(AppCommand::ToggleEditorMode), key_style), Span::styled("Switch to Vim", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Nano aliases", subheader_style)),
            Line::from(vec![Span::styled(" Ctrl+k    ", key_style), Span::styled("Cut selection or line", desc_style)]),
            Line::from(vec![Span::styled(" Ctrl+u    ", key_style), Span::styled("Paste", desc_style)]),
            Line::from(""),
        ]
    } else {
        vec![
            Line::from(""),
            Line::from(Span::styled(" Edit Mode - Normal", header_style)),
            Line::from(""),
            Line::from(Span::styled("  Mode Changes", subheader_style)),
            Line::from(vec![Span::styled(" i/a       ", key_style), Span::styled("Insert before/after cursor", desc_style)]),
            Line::from(vec![Span::styled(" I/A       ", key_style), Span::styled("Insert at line start/end", desc_style)]),
            Line::from(vec![Span::styled(" o/O       ", key_style), Span::styled("New line below/above", desc_style)]),
            Line::from(vec![Span::styled(" v/V       ", key_style), Span::styled("Visual / Visual Line mode", desc_style)]),
            Line::from(vec![Span::styled(" :         ", key_style), Span::styled("Command mode", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Movement", subheader_style)),
            Line::from(vec![Span::styled(" h/j/k/l   ", key_style), Span::styled("Move left/down/up/right", desc_style)]),
            Line::from(vec![Span::styled(" w/W       ", key_style), Span::styled("Word/WORD forward", desc_style)]),
            Line::from(vec![Span::styled(" b/B       ", key_style), Span::styled("Word/WORD backward", desc_style)]),
            Line::from(vec![Span::styled(" e/E       ", key_style), Span::styled("Word/WORD end forward", desc_style)]),
            Line::from(vec![Span::styled(" 0/^/$     ", key_style), Span::styled("Line start/first char/end", desc_style)]),
            Line::from(vec![Span::styled(" gg/G      ", key_style), Span::styled("File top/bottom", desc_style)]),
            Line::from(vec![Span::styled(" {/}       ", key_style), Span::styled("Paragraph backward/forward", desc_style)]),
            Line::from(vec![Span::styled(" %         ", key_style), Span::styled("Matching bracket", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Find Character", subheader_style)),
            Line::from(vec![Span::styled(" f/F{c}    ", key_style), Span::styled("Find char forward/backward", desc_style)]),
            Line::from(vec![Span::styled(" t/T{c}    ", key_style), Span::styled("Till char forward/backward", desc_style)]),
            Line::from(vec![Span::styled(" ;/,       ", key_style), Span::styled("Repeat find / reverse", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Operators (+ motion/text obj)", subheader_style)),
            Line::from(vec![Span::styled(" d{motion} ", key_style), Span::styled("Delete", desc_style)]),
            Line::from(vec![Span::styled(" c{motion} ", key_style), Span::styled("Change (delete + insert)", desc_style)]),
            Line::from(vec![Span::styled(" y{motion} ", key_style), Span::styled("Yank (copy)", desc_style)]),
            Line::from(vec![Span::styled(" >/< {m}   ", key_style), Span::styled("Indent / Outdent", desc_style)]),
            Line::from(vec![Span::styled(" dd/cc/yy  ", key_style), Span::styled("Operate on whole line", desc_style)]),
            Line::from(vec![Span::styled(" D/C/Y     ", key_style), Span::styled("Operate to line end", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Text Objects (inner/around)", subheader_style)),
            Line::from(vec![Span::styled(" iw/aw     ", key_style), Span::styled("Inner/around word", desc_style)]),
            Line::from(vec![Span::styled(" iW/aW     ", key_style), Span::styled("Inner/around WORD", desc_style)]),
            Line::from(vec![Span::styled(" i\"/a\"     ", key_style), Span::styled("Inner/around double quotes", desc_style)]),
            Line::from(vec![Span::styled(" i'/a'     ", key_style), Span::styled("Inner/around single quotes", desc_style)]),
            Line::from(vec![Span::styled(" i(/a(     ", key_style), Span::styled("Inner/around parentheses", desc_style)]),
            Line::from(vec![Span::styled(" i[/a[     ", key_style), Span::styled("Inner/around brackets", desc_style)]),
            Line::from(vec![Span::styled(" i{/a{     ", key_style), Span::styled("Inner/around braces", desc_style)]),
            Line::from(vec![Span::styled(" ip/ap     ", key_style), Span::styled("Inner/around paragraph", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Actions", subheader_style)),
            Line::from(vec![Span::styled(" x/X       ", key_style), Span::styled("Delete char fwd/back", desc_style)]),
            Line::from(vec![Span::styled(" s/S       ", key_style), Span::styled("Substitute char/line", desc_style)]),
            Line::from(vec![Span::styled(" r{c}      ", key_style), Span::styled("Replace char", desc_style)]),
            Line::from(vec![Span::styled(" J         ", key_style), Span::styled("Join lines", desc_style)]),
            Line::from(vec![Span::styled(" p/P       ", key_style), Span::styled("Paste after/before", desc_style)]),
            Line::from(vec![Span::styled(" u/Ctrl+r  ", key_style), Span::styled("Undo / Redo", desc_style)]),
            Line::from(vec![Span::styled(" .         ", key_style), Span::styled("Repeat last command", desc_style)]),
            Line::from(vec![Span::styled(" ~         ", key_style), Span::styled("Toggle case", desc_style)]),
            Line::from(vec![Span::styled(keys(AppCommand::InsertTask), key_style), Span::styled("Insert task item", desc_style)]),
            Line::from(vec![Span::styled(keys(AppCommand::ToggleEditorFold), key_style), Span::styled("Toggle heading fold", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Registers", subheader_style)),
            Line::from(vec![Span::styled(" \"{reg}    ", key_style), Span::styled("Use register (a-z, 0-9)", desc_style)]),
            Line::from(vec![Span::styled(" \"+/\"*     ", key_style), Span::styled("System clipboard", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Macros", subheader_style)),
            Line::from(vec![Span::styled(" q{reg}    ", key_style), Span::styled("Record macro to register", desc_style)]),
            Line::from(vec![Span::styled(" q         ", key_style), Span::styled("Stop recording", desc_style)]),
            Line::from(vec![Span::styled(" @{reg}    ", key_style), Span::styled("Play macro from register", desc_style)]),
            Line::from(vec![Span::styled(" @@        ", key_style), Span::styled("Repeat last macro", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Marks", subheader_style)),
            Line::from(vec![Span::styled(" m{a-z}    ", key_style), Span::styled("Set mark", desc_style)]),
            Line::from(vec![Span::styled(" '{a-z}    ", key_style), Span::styled("Jump to mark (line)", desc_style)]),
            Line::from(vec![Span::styled(" `{a-z}    ", key_style), Span::styled("Jump to mark (exact)", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Scrolling", subheader_style)),
            Line::from(vec![Span::styled(" Ctrl+u/d  ", key_style), Span::styled("Half page up/down", desc_style)]),
            Line::from(vec![Span::styled(" Ctrl+b/f  ", key_style), Span::styled("Full page up/down", desc_style)]),
            Line::from(vec![Span::styled(" za/zM/zR  ", key_style), Span::styled("Toggle/fold all/unfold all", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Count Prefix", subheader_style)),
            Line::from(vec![Span::styled(" {n}{cmd}  ", key_style), Span::styled("Repeat cmd n times", desc_style)]),
            Line::from(vec![Span::styled(" 5j, 3w    ", key_style), Span::styled("Move 5 down, 3 words", desc_style)]),
            Line::from(vec![Span::styled(" 2dd       ", key_style), Span::styled("Delete 2 lines", desc_style)]),
            Line::from(""),
            Line::from(Span::styled("  Save/Exit", subheader_style)),
            Line::from(vec![Span::styled(" Ctrl+s    ", key_style), Span::styled("Save and exit", desc_style)]),
            Line::from(vec![Span::styled(" Esc       ", key_style), Span::styled("Exit to normal / cancel", desc_style)]),
            Line::from(vec![Span::styled(" :w/:q/:wq ", key_style), Span::styled("Write/Quit/Both", desc_style)]),
            Line::from(""),
        ]
    };
    let left_total = left_content.len();
    let right_total = right_content.len();
    let max_total = left_total.max(right_total);
    let visible_height = inner_area.height as usize;
    let max_scroll = max_total.saturating_sub(visible_height);
    let scroll = app.state.help_scroll.min(max_scroll);
    let left_visible: Vec<Line> = left_content.into_iter().skip(scroll).take(visible_height).collect();
    let right_visible: Vec<Line> = right_content.into_iter().skip(scroll).take(visible_height).collect();
    let left_paragraph = Paragraph::new(left_visible).alignment(Alignment::Left);
    let right_paragraph = Paragraph::new(right_visible).alignment(Alignment::Left);
    f.render_widget(left_paragraph, columns[0]);
    f.render_widget(right_paragraph, columns[1]);
    scroll
}

pub fn render_directory_not_found_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.state.theme;
    let dialog_width = 58.min(area.width.saturating_sub(4));
    let dialog_height = 14.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let content = vec![
        Line::from(""),
        Line::from(Span::styled("Directory Not Found", Style::default().fg(theme.error).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled("The configured notes directory does not exist:", Style::default().fg(theme.foreground))),
        Line::from(Span::styled(&app.state.config.notes_dir, Style::default().fg(theme.warning))),
        Line::from(""),
        Line::from(Span::styled("Would you like to create it?", Style::default().fg(theme.foreground))),
        Line::from(""),
        Line::from(vec![
            Span::styled("c", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
            Span::styled(" Create directory  ", Style::default().fg(theme.foreground)),
            Span::styled("q", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)),
            Span::styled(" Quit and fix config", Style::default().fg(theme.foreground)),
        ]),
        Line::from(""),
        Line::from(Span::styled("Config: ~/.config/ekphos/config.toml", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))),
    ];
    let dialog = Paragraph::new(content).block(Block::default().title(" Error ").borders(Borders::ALL).border_style(Style::default().fg(theme.error)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
    f.render_widget(dialog, dialog_area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_dialog_lists_every_app_command_once() {
        let mut listed = HELP_SECTIONS.iter().flat_map(|section| section.entries).flat_map(|entry| entry.commands.iter().copied()).collect::<Vec<_>>();
        let mut expected = AppCommand::ALL.to_vec();
        listed.sort_unstable();
        expected.sort_unstable();
        assert_eq!(listed, expected, "help sections must contain every registered app command exactly once");
    }
}
