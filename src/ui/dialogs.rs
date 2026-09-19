use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::App;
use crate::config::{EditingMode, Theme};
use crate::keybindings::{AppCommand, KeybindingFallback};

const TITLE_MAIN: &[&str] = &["████████ ██   ██ ██████  ██   ██  ██████  ███████", "██       ██  ██  ██   ██ ██   ██ ██    ██ ██     ", "█████    █████   ██████  ███████ ██    ██ ███████", "██       ██  ██  ██      ██   ██ ██    ██      ██", "████████ ██   ██ ██      ██   ██  ██████  ███████"];
const CHANGELOG: &str = include_str!("../../CHANGELOG.md");
const ANNOUNCEMENT_HORIZONTAL_PADDING: usize = 2;

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
            HelpEntry { commands: &[AppCommand::ShowChangelog], description: "Show what's new" },
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
            HelpEntry { commands: &[AppCommand::CreateDocument], description: "Create document" },
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
        title: "Canvas",
        entries: &[
            HelpEntry { commands: &[AppCommand::CanvasSelectLeft, AppCommand::CanvasSelectRight], description: "Select card left / right" },
            HelpEntry { commands: &[AppCommand::CanvasZoomIn, AppCommand::CanvasZoomOut], description: "Zoom Canvas in / out" },
            HelpEntry { commands: &[AppCommand::CanvasUndo, AppCommand::CanvasRedo], description: "Undo / redo Canvas edit" },
            HelpEntry { commands: &[AppCommand::ToggleCanvasShortcuts], description: "Show / hide Canvas shortcuts" },
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

fn release_notes<'a>(markdown: &'a str, version: &str) -> Option<&'a str> {
    let heading = format!("## [{version}]");
    let heading_start = markdown.find(&heading)?;
    let after_heading = &markdown[heading_start + heading.len()..];
    let body_start = after_heading.find('\n').map_or(0, |index| index + 1);
    let body = &after_heading[body_start..];
    let body_end = body.find("\n## [").unwrap_or(body.len());
    Some(body[..body_end].trim())
}

fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut wrapped = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let next_width = unicode_width::UnicodeWidthStr::width(current.as_str()) + usize::from(!current.is_empty()) + unicode_width::UnicodeWidthStr::width(word);
        if !current.is_empty() && next_width > width {
            wrapped.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() || wrapped.is_empty() {
        wrapped.push(current);
    }
    wrapped
}

struct ChangelogLine {
    line: Line<'static>,
    announcement: bool,
    link: Option<(usize, usize, String)>,
}

fn external_url_range(text: &str) -> Option<(usize, usize)> {
    let start = ["https://", "http://"].into_iter().filter_map(|scheme| text.find(scheme)).min()?;
    let mut end = start + text[start..].find(char::is_whitespace).unwrap_or(text.len() - start);
    while let Some(last) = text[..end].chars().next_back().filter(|character| ".,;:!?)]}".contains(*character)) {
        end -= last.len_utf8();
    }
    (end > start).then_some((start, end))
}

fn changelog_text_line(prefix: &str, prefix_style: Style, text: String, text_style: Style, link_color: ratatui::style::Color, announcement: bool) -> ChangelogLine {
    let Some((url_start, url_end)) = external_url_range(&text) else {
        return ChangelogLine { line: Line::from(vec![Span::styled(prefix.to_string(), prefix_style), Span::styled(text, text_style)]), announcement, link: None };
    };
    let before = &text[..url_start];
    let url = &text[url_start..url_end];
    let after = &text[url_end..];
    let link_start = unicode_width::UnicodeWidthStr::width(prefix) + unicode_width::UnicodeWidthStr::width(before);
    let link_width = unicode_width::UnicodeWidthStr::width(url);
    ChangelogLine {
        line: Line::from(vec![Span::styled(prefix.to_string(), prefix_style), Span::styled(before.to_string(), text_style), Span::styled(url.to_string(), text_style.fg(link_color).add_modifier(Modifier::UNDERLINED)), Span::styled(after.to_string(), text_style)]),
        announcement,
        link: Some((link_start, link_width, url.to_string())),
    }
}

fn changelog_lines(notes: &str, theme: &Theme, width: u16) -> Vec<ChangelogLine> {
    let mut lines = Vec::new();
    let mut announcement = false;
    let width = width as usize;
    for source in notes.lines() {
        let line = source.trim();
        if let Some(title) = line.strip_prefix("### ") {
            announcement = title == "Announcement";
            if !lines.is_empty() {
                lines.push(ChangelogLine { line: Line::from(""), announcement, link: None });
            }
            if announcement {
                lines.push(ChangelogLine { line: Line::from(""), announcement: true, link: None });
            }
            let color = if announcement { theme.warning } else { theme.dialog.title };
            let prefix = if announcement { " ".repeat(ANNOUNCEMENT_HORIZONTAL_PADDING) } else { String::new() };
            lines.push(ChangelogLine { line: Line::from(vec![Span::raw(prefix), Span::styled(title.to_string(), Style::default().fg(color).add_modifier(Modifier::BOLD))]), announcement, link: None });
        } else if let Some(item) = line.strip_prefix("- ") {
            let marker_color = if announcement { theme.warning } else { theme.primary };
            let card_padding = if announcement { ANNOUNCEMENT_HORIZONTAL_PADDING * 2 } else { 0 };
            for (index, part) in wrap_words(item, width.saturating_sub(2 + card_padding)).into_iter().enumerate() {
                let marker = match (announcement, index) {
                    (true, 0) => "  • ",
                    (true, _) => "    ",
                    (false, 0) => "• ",
                    (false, _) => "  ",
                };
                lines.push(changelog_text_line(marker, Style::default().fg(marker_color), part, Style::default().fg(theme.dialog.text), theme.content.link, announcement));
            }
        } else if line.is_empty() {
            if lines.last().is_some_and(|line| !line.line.spans.is_empty()) {
                lines.push(ChangelogLine { line: Line::from(""), announcement, link: None });
            }
        } else {
            let (prefix, text_width) = if announcement { ("  ", width.saturating_sub(ANNOUNCEMENT_HORIZONTAL_PADDING * 2)) } else { ("", width) };
            lines.extend(wrap_words(line, text_width).into_iter().map(|part| changelog_text_line(prefix, Style::default(), part, Style::default().fg(theme.dialog.text), theme.content.link, announcement)));
        }
    }
    while lines.last().is_some_and(|line| line.line.spans.is_empty()) {
        lines.pop();
    }
    lines
}

pub struct ChangelogDialogRender {
    pub scroll: usize,
    pub links: Vec<(Rect, String)>,
}

pub fn render_changelog_dialog(f: &mut Frame, app: &App) -> ChangelogDialogRender {
    render_changelog_dialog_for_version(f, app, env!("CARGO_PKG_VERSION"))
}

pub(crate) fn render_changelog_dialog_for_version(f: &mut Frame, app: &App, version: &str) -> ChangelogDialogRender {
    let area = f.area();
    let theme = &app.state.theme;
    let dialog_width = 76.min(area.width.saturating_sub(4));
    let dialog_height = 22.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let title = format!(" What's new in Ekphos v{version} ");
    let block = Block::default().title(title).borders(Borders::ALL).border_style(Style::default().fg(theme.dialog.border)).style(Style::default().bg(theme.dialog.background));
    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);
    let padded = inner.inner(Margin { horizontal: 2, vertical: 1 });
    let sections = Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(0), Constraint::Length(1), Constraint::Length(3)]).split(padded);
    let notes = release_notes(CHANGELOG, version).or_else(|| release_notes(CHANGELOG, "Unreleased")).unwrap_or("No summary is available for this version.");
    let lines = changelog_lines(notes, theme, sections[0].width);
    let max_scroll = lines.len().saturating_sub(sections[0].height as usize);
    let scroll = app.state.changelog_scroll.min(max_scroll);
    let mut links = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let visible_row = index.checked_sub(scroll).filter(|row| *row < sections[0].height as usize);
        if line.announcement {
            if let Some(row) = visible_row {
                let highlight_area = Rect::new(sections[0].x, sections[0].y + row as u16, sections[0].width, 1);
                f.render_widget(Block::default().style(Style::default().bg(theme.flat.surface_raised)), highlight_area);
            }
        }
        if let Some((start, width, url)) = &line.link {
            let link_area = visible_row.map_or_else(Rect::default, |row| {
                let x = sections[0].x.saturating_add((*start).min(u16::MAX as usize) as u16);
                Rect::new(x, sections[0].y + row as u16, (*width).min(sections[0].right().saturating_sub(x) as usize) as u16, 1)
            });
            links.push((link_area, url.clone()));
        }
    }
    f.render_widget(Paragraph::new(lines.into_iter().map(|line| line.line).collect::<Vec<_>>()).scroll((scroll.min(u16::MAX as usize) as u16, 0)), sections[0]);
    let reopen_key = app.state.keymap.binding_label(AppCommand::ShowChangelog);
    let reopen_hint = if reopen_key == "Unbound" { "Remap show_changelog to reopen".to_string() } else { format!("{reopen_key} Reopen") };
    let footer = vec![
        Line::from(Span::styled("o Open Discord · Click the underlined link", Style::default().fg(theme.muted))),
        Line::from(Span::styled("↑/↓ or j/k Scroll · PgUp/PgDn Page", Style::default().fg(theme.muted))),
        Line::from(Span::styled(format!("Enter / Esc / q Close · {reopen_hint}"), Style::default().fg(theme.muted))),
    ];
    f.render_widget(Paragraph::new(footer).alignment(Alignment::Center), sections[2]);
    ChangelogDialogRender { scroll, links }
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

pub fn render_create_document_dialog(f: &mut Frame, app: &App, kind: crate::vault::VaultFileKind) {
    let area = f.area();
    let theme = &app.state.theme;
    let has_context = app.vault.target_folder.is_some();
    let has_error = app.state.dialog_error.is_some();
    let base_height = if has_context { 12 } else { 11 };
    let dialog_height = if has_error { base_height + 2 } else { base_height };
    let dialog_width = 62.min(area.width.saturating_sub(4));
    let dialog_height = dialog_height.min(area.height.saturating_sub(4));
    let dialog_area = Rect { x: (area.width.saturating_sub(dialog_width)) / 2, y: (area.height.saturating_sub(dialog_height)) / 2, width: dialog_width, height: dialog_height };
    f.render_widget(Clear, dialog_area);
    let mut type_spans = vec![Span::styled("Type: ", Style::default().fg(theme.foreground))];
    for candidate in [crate::vault::VaultFileKind::Markdown, crate::vault::VaultFileKind::Canvas, crate::vault::VaultFileKind::Base] {
        let style = if candidate == kind { Style::default().fg(theme.background).bg(theme.selection).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.muted) };
        type_spans.push(Span::styled(format!(" {} ", candidate.display_name()), style));
        type_spans.push(Span::raw(" "));
    }
    let mut content = vec![Line::from(""), Line::from(type_spans), Line::from(""), Line::from(Span::styled("Enter document name:", Style::default().fg(theme.foreground)))];
    if let Some(ref folder_path) = app.vault.target_folder {
        if let Some(folder_name) = folder_path.file_name() {
            content.push(Line::from(Span::styled(format!("in {}/", folder_name.to_string_lossy()), Style::default().fg(theme.info))));
        }
    }
    content.push(Line::from(""));
    let suffix = format!(".{}", kind.extension());
    let shown_suffix = if app.state.input_buffer.trim_end().ends_with(&suffix) { String::new() } else { suffix };
    content.push(Line::from(vec![Span::styled("> ", Style::default().fg(theme.warning)), Span::styled(&app.state.input_buffer, Style::default().fg(theme.foreground)), Span::styled("█", Style::default().fg(theme.cursor)), Span::styled(shown_suffix, Style::default().fg(theme.info))]));
    if let Some(ref error) = app.state.dialog_error {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(error.as_str(), Style::default().fg(theme.error))));
    }
    content.push(Line::from(""));
    content.push(Line::from(Span::styled("Tab/←/→: Type  Enter: Create  Esc: Cancel", Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC))));
    let border_color = if has_error { theme.error } else { theme.success };
    let dialog = Paragraph::new(content).block(Block::default().title(" New Document ").borders(Borders::ALL).border_style(Style::default().fg(border_color)).style(Style::default().bg(theme.background))).alignment(Alignment::Center);
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
        Line::from(Span::styled("This directory is empty", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled("No supported documents found in:", Style::default().fg(theme.foreground))),
        Line::from(Span::styled(&app.state.config.notes_dir, Style::default().fg(theme.muted))),
        Line::from(""),
        Line::from(Span::styled(format!("Press {} to create your first document", app.state.keymap.binding_label(AppCommand::CreateDocument)), Style::default().fg(theme.success))),
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
            Line::from(vec![Span::styled(keys(AppCommand::ToggleEditorFold), key_style), Span::styled("Fold heading; indent list or insert tab", desc_style)]),
            Line::from(vec![Span::styled(" Shift+Tab ", key_style), Span::styled("Outdent list item", desc_style)]),
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

    #[test]
    fn current_release_has_release_notes() {
        assert!(release_notes(CHANGELOG, env!("CARGO_PKG_VERSION")).is_some());
    }

    #[test]
    fn release_notes_keep_an_optional_announcement_before_the_summary() {
        let notes = release_notes(CHANGELOG, "0.50.10").expect("fixture release must have notes");
        let announcement = notes.find("### Announcement").expect("current release should exercise announcement rendering");
        let summary = notes.find("### Summary").expect("current release needs a concise summary");
        assert!(announcement < summary);
    }

    #[test]
    fn changelog_copy_wraps_to_the_available_width() {
        let wrapped = wrap_words("A concise release summary for narrow terminals", 16);
        assert!(wrapped.len() > 1);
        assert!(wrapped.iter().all(|line| unicode_width::UnicodeWidthStr::width(line.as_str()) <= 16));
    }
}
