use std::io;

use crossterm::cursor::SetCursorStyle;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};

use crate::app::{App, BlockInsertMode, BlockInsertState, ContextMenuItem, ContextMenuState, DeleteType, DialogState, Focus, LinkInfo, Mode, SearchPickerState, SidebarItemKind, TaskFilterKind, WikiAutocompleteMode, WikiAutocompleteState};
use crate::clipboard::{self, ClipboardContent};
use crate::config::{Config, EditingMode};
use crate::editor::{CursorMove, CursorShape, Position};
use crate::keybindings::{AppCommand, KeyResolution};
use crate::ui;
use crate::vim::command::{parse_command, Command};
use crate::vim::{FindState, PendingFind, PendingMacro, PendingMark, TextObject, TextObjectScope, VimInputMode, VimMode};

mod commands;
mod dialogs;
mod edit;
mod event_loop;
mod graph;
mod helix;
mod mouse;
mod search;
mod standard;
mod vim_modes;
mod vim_normal;

use commands::*;
use dialogs::*;
use edit::*;
pub use event_loop::run_app;
use event_loop::{open_selected_content_target, update_cursor_style};
use graph::*;
use helix::*;
use mouse::*;
use search::*;
use standard::*;
use vim_modes::*;
use vim_normal::*;
