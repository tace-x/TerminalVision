//! Reusable modal dialogs and command palette UI.
//!
//! Renders modal overlays (Confirm, Input/Create, Rename, Help, Command Palette)
//! centered within the terminal frame.
//!
//! The UI layer only reads prepared application state and never performs filesystem
//! operations or mutates domain models directly.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::app::modes::Mode;
use crate::app::state::{App, CreateKind};
use crate::ui::theme::Theme;
use crate::ui::truncate_to_width;

/// Calculates a centered rectangle of at most `max_width` by `max_height` inside `area`.
pub fn centered_rect(max_width: u16, max_height: u16, area: Rect) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect::default();
    }

    let width = if area.width <= 2 {
        area.width
    } else {
        max_width.min(area.width.saturating_sub(2)).max(1)
    };

    let height = if area.height <= 2 {
        area.height
    } else {
        max_height.min(area.height.saturating_sub(2)).max(1)
    };

    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;

    Rect::new(x, y, width, height)
}

/// Renders the active modal dialog or command palette on top of the existing interface.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.width < 4 || area.height < 4 {
        return;
    }

    match app.mode() {
        Mode::Confirm => render_confirm_dialog(frame, area, app),
        Mode::Create | Mode::Rename => render_input_dialog(frame, area, app),
        Mode::CommandPalette => render_command_palette(frame, area, app),
        Mode::Help => render_help_dialog(frame, area, app),
        Mode::Bookmarks => render_bookmarks_dialog(frame, area, app),
        Mode::Jump => render_jump_dialog(frame, area, app),
        Mode::SmartJump => render_smart_jump_dialog(frame, area, app),
        Mode::ProjectCockpit => render_project_cockpit_dialog(frame, area, app),
        Mode::GitStatusPanel => render_git_status_panel_dialog(frame, area, app),
        Mode::FileRadar => render_file_radar_dialog(frame, area, app),
        Mode::RevealContext => render_reveal_context_dialog(frame, area, app),
        _ => {}
    }
}

/// Renders a confirmation prompt dialog (e.g. Delete confirmation).
fn render_confirm_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = centered_rect(54, 10, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = Theme::default();
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Confirm Action ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.dialog_border_confirm);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let active_pane = app.pane(app.active_pane());
    let selected_count = active_pane.selected_count();
    let selected_entry = active_pane.selected_entry();

    let message = if selected_count > 1 {
        format!("Permanently delete {selected_count} selected items?")
    } else if let Some(entry) = selected_entry {
        let entry_name = entry.name().to_str().unwrap_or("this item");
        let max_name_width = (inner.width as usize).saturating_sub(24);
        let display_name = truncate_to_width(entry_name, max_name_width);
        if entry.is_dir() {
            format!("Delete folder \"{display_name}\" and all its contents?")
        } else {
            format!("Are you sure you want to delete \"{display_name}\"?")
        }
    } else {
        "Are you sure you want to delete the selected item?".to_string()
    };
    let is_confirm = app.confirm_selection();

    // Visual buttons with explicit indicators for keyboard focus
    let yes_button = if is_confirm {
        Span::styled(" ▶ [ Yes ] ◀ ", theme.dialog_button_active)
    } else {
        Span::styled("   [ Yes ]   ", theme.dialog_button_inactive)
    };

    let no_button = if !is_confirm {
        Span::styled(" ▶ [ No ] ◀ ", theme.dialog_button_active)
    } else {
        Span::styled("   [ No ]   ", theme.dialog_button_inactive)
    };

    if inner.height <= 2 {
        // Highly compact mode for short terminals
        let line = Line::from(vec![Span::styled(
            truncate_to_width(&message, inner.width as usize),
            theme.dialog_message,
        )]);
        frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), inner);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Top margin
            Constraint::Length(2), // Message
            Constraint::Length(1), // Buttons
            Constraint::Min(1),    // Footer
        ])
        .split(inner);

    let msg_paragraph = Paragraph::new(Line::from(Span::styled(message, theme.dialog_message)))
        .alignment(Alignment::Center);
    frame.render_widget(msg_paragraph, chunks[1]);

    let buttons_line = Line::from(vec![yes_button, Span::raw("   "), no_button]);
    let buttons_paragraph = Paragraph::new(buttons_line).alignment(Alignment::Center);
    frame.render_widget(buttons_paragraph, chunks[2]);

    if chunks[3].height > 0 {
        let hint = Paragraph::new(Line::from(Span::styled(
            "←/→: Select  •  Enter: Confirm  •  Esc: Cancel",
            theme.dialog_hint,
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[3]);
    }
}

/// Renders a text input dialog for creating or renaming entries.
fn render_input_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = centered_rect(56, 8, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = Theme::default();
    frame.render_widget(Clear, dialog_area);

    let (title, prompt) = match (app.mode(), app.create_kind()) {
        (Mode::Create, Some(CreateKind::Directory)) => (" New Directory ", "Directory name:"),
        (Mode::Create, _) => (" New File ", "File name:"),
        (Mode::Rename, _) => (" Rename Entry ", "New name:"),
        _ => (" Input ", "Name:"),
    };

    let block = Block::default()
        .title(title)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.dialog_border);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let buffer = app.input_buffer();
    let cursor_pos = app.cursor_position();

    // Render input field with visible cursor
    let mut spans = Vec::new();
    spans.push(Span::styled(format!("{prompt} "), theme.dialog_message));

    let chars: Vec<char> = buffer.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if i == cursor_pos {
            spans.push(Span::styled(ch.to_string(), theme.dialog_button_active));
        } else {
            spans.push(Span::styled(ch.to_string(), theme.dialog_input));
        }
    }
    if cursor_pos >= chars.len() {
        spans.push(Span::styled(" ", theme.dialog_button_active));
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Top margin
            Constraint::Length(1), // Input field
            Constraint::Min(1),    // Footer
        ])
        .split(inner);

    let input_paragraph = Paragraph::new(Line::from(spans));
    frame.render_widget(input_paragraph, chunks[1]);

    if chunks[2].height > 0 {
        let hint = Paragraph::new(Line::from(Span::styled(
            "Enter: Confirm  •  Esc: Cancel  •  ←/→: Move Cursor",
            theme.dialog_hint,
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[2]);
    }
}

/// Renders the path-jump dialog for `Mode::Jump`.
///
/// A compact text input centred over the panes where the user can type any
/// absolute or relative path and press Enter to navigate directly to it.
fn render_jump_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = centered_rect(64, 7, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = Theme::default();
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Go to Path ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.dialog_border);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let buffer = app.input_buffer();
    let cursor_pos = app.cursor_position();

    // Render input field with cursor indicator.
    let prompt = "Path: ";
    let prompt_width = crate::ui::display_width(prompt);
    let available_width = (inner.width as usize).saturating_sub(prompt_width);

    // Scroll the visible window if the text is wider than the field.
    let chars: Vec<char> = buffer.chars().collect();
    let visible_start = if cursor_pos > available_width {
        cursor_pos.saturating_sub(available_width)
    } else {
        0
    };
    let visible_end = (visible_start + available_width).min(chars.len());

    let mut spans = vec![Span::styled(prompt, theme.dialog_message)];
    for i in visible_start..=visible_end {
        if i == cursor_pos {
            if i < chars.len() {
                spans.push(Span::styled(
                    chars[i].to_string(),
                    theme.dialog_button_active,
                ));
            } else {
                spans.push(Span::styled(" ", theme.dialog_button_active));
            }
        } else if i < chars.len() {
            spans.push(Span::styled(chars[i].to_string(), theme.dialog_input));
        }
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Top margin
            Constraint::Length(1), // Input field
            Constraint::Min(1),    // Footer hint
        ])
        .split(inner);

    frame.render_widget(Paragraph::new(Line::from(spans)), chunks[1]);

    if chunks[2].height > 0 {
        let hint = Paragraph::new(Line::from(Span::styled(
            "Enter: Jump  •  Esc: Cancel  •  ←/→: Move Cursor",
            theme.dialog_hint,
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[2]);
    }
}

/// Renders the searchable command palette overlay.
fn render_command_palette(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = centered_rect(64, 18, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = Theme::default();
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Command Palette (Ctrl+P) ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.palette_shortcut);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let palette = app.command_palette();
    let query = palette.query();
    let selected = palette.selected_index();
    let commands = palette.filtered_commands();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Query input box
            Constraint::Length(1), // Separator
            Constraint::Min(1),    // Results list
            Constraint::Length(1), // Footer hint
        ])
        .split(inner);

    // 1. Query input box
    let query_line = Line::from(vec![
        Span::styled(" > ", theme.palette_shortcut),
        Span::styled(query, theme.palette_input),
        Span::styled("█", theme.palette_shortcut),
    ]);
    frame.render_widget(Paragraph::new(query_line), chunks[0]);

    // 2. Separator line
    let sep = "─".repeat(chunks[1].width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(sep, theme.dialog_hint)),
        chunks[1],
    );

    // 3. Command list
    let list_height = chunks[2].height as usize;
    let list_width = chunks[2].width as usize;
    let mut command_lines = Vec::new();

    if commands.is_empty() {
        command_lines.push(Line::from(Span::styled(
            "   No matching commands",
            theme.empty_state_text,
        )));
    } else {
        // Calculate window to scroll with selection
        let scroll_offset = if selected >= list_height {
            selected.saturating_sub(list_height - 1)
        } else {
            0
        };

        let badge_width = 8;
        let name_col_width = (18).min(list_width.saturating_sub(badge_width + 8).max(6));
        let avail_desc = list_width.saturating_sub(badge_width + name_col_width + 10);

        for (idx, cmd) in commands
            .iter()
            .skip(scroll_offset)
            .take(list_height)
            .enumerate()
        {
            let actual_idx = scroll_offset + idx;
            let is_selected = actual_idx == selected;

            let badge = match cmd.category() {
                crate::app::actions::ActionCategory::Navigation => "[NAV]",
                crate::app::actions::ActionCategory::Files => "[FILES]",
                crate::app::actions::ActionCategory::Search => "[SEARCH]",
                crate::app::actions::ActionCategory::Tabs => "[TABS]",
                crate::app::actions::ActionCategory::Bookmarks => "[BOOK]",
                crate::app::actions::ActionCategory::View => "[VIEW]",
                crate::app::actions::ActionCategory::Preview => "[PREV]",
                crate::app::actions::ActionCategory::Git => "[GIT]",
                crate::app::actions::ActionCategory::Project => "[PROJ]",
                crate::app::actions::ActionCategory::Terminal => "[TERM]",
                crate::app::actions::ActionCategory::Application => "[APP]",
            };

            let formatted_badge = format!("{:<8}", badge);
            let truncated_name = truncate_to_width(cmd.name(), name_col_width);
            let formatted_name = format!("{:<width$}", truncated_name, width = name_col_width);

            let mut spans = Vec::new();
            if is_selected {
                spans.push(Span::styled(" ▶ ", theme.palette_shortcut));
                spans.push(Span::styled(formatted_badge, theme.palette_shortcut));
                spans.push(Span::styled(formatted_name, theme.palette_selected));
                if avail_desc > 0 {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        truncate_to_width(cmd.description(), avail_desc),
                        theme.palette_description,
                    ));
                }
                if let Some(sc) = cmd.shortcut() {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(format!("[{sc}]"), theme.palette_shortcut));
                }
            } else {
                spans.push(Span::raw("   "));
                spans.push(Span::styled(formatted_badge, theme.dialog_hint));
                spans.push(Span::styled(formatted_name, theme.palette_unselected));
                if avail_desc > 0 {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        truncate_to_width(cmd.description(), avail_desc),
                        theme.dialog_hint,
                    ));
                }
                if let Some(sc) = cmd.shortcut() {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(format!("[{sc}]"), theme.dialog_hint));
                }
            }
            command_lines.push(Line::from(spans));
        }
    }

    frame.render_widget(Paragraph::new(command_lines), chunks[2]);

    // 4. Footer
    let hint = Line::from(Span::styled(
        "↑/↓: Navigate  •  Enter: Run  •  Esc: Cancel",
        theme.dialog_hint,
    ));
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), chunks[3]);
}

/// Renders the help / keyboard shortcuts reference dialog.
fn render_help_dialog(frame: &mut Frame, area: Rect, _app: &App) {
    let dialog_area = centered_rect(80, 22, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = Theme::default();
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Help & Keyboard Shortcuts ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.help_category);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let build_category_from_commands =
        |cat: crate::app::actions::ActionCategory, max_w: usize| -> Vec<Line<'static>> {
            let mut lines = Vec::new();
            lines.push(Line::from(Span::styled(
                format!("── {} ──", cat.display_name()),
                theme.help_category,
            )));
            let key_col_w = (14).min(max_w.saturating_sub(4).max(4));
            let avail_desc = max_w.saturating_sub(key_col_w + 3);
            for cmd in crate::commands::palette::Command::ALL {
                if cmd.category() == cat
                    && let Some(sc) = cmd.shortcut()
                {
                    let key_str = format!(
                        "  {:<width$}",
                        truncate_to_width(sc, key_col_w),
                        width = key_col_w
                    );
                    let desc_str = truncate_to_width(cmd.description(), avail_desc);
                    lines.push(Line::from(vec![
                        Span::styled(key_str, theme.help_key),
                        Span::styled(desc_str, theme.help_desc),
                    ]));
                }
            }
            lines.push(Line::raw(""));
            lines
        };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Shortcuts list
            Constraint::Length(1), // Mouse hint
            Constraint::Length(1), // Footer hint
        ])
        .split(inner);

    if inner.width >= 70 {
        let half_w = (inner.width as usize) / 2;
        let mut left_lines =
            build_category_from_commands(crate::app::actions::ActionCategory::Navigation, half_w);
        left_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Files,
            half_w,
        ));
        left_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Tabs,
            half_w,
        ));
        left_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Bookmarks,
            half_w,
        ));
        left_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Search,
            half_w,
        ));

        let mut right_lines =
            build_category_from_commands(crate::app::actions::ActionCategory::View, half_w);
        right_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Preview,
            half_w,
        ));
        right_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Git,
            half_w,
        ));
        right_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Project,
            half_w,
        ));
        right_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Application,
            half_w,
        ));

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0]);
        frame.render_widget(Paragraph::new(left_lines), columns[0]);
        frame.render_widget(Paragraph::new(right_lines), columns[1]);
    } else {
        let full_w = inner.width as usize;
        let mut all_lines =
            build_category_from_commands(crate::app::actions::ActionCategory::Navigation, full_w);
        all_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Files,
            full_w,
        ));
        all_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Tabs,
            full_w,
        ));
        all_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Bookmarks,
            full_w,
        ));
        all_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Search,
            full_w,
        ));
        all_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::View,
            full_w,
        ));
        all_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Preview,
            full_w,
        ));
        all_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Git,
            full_w,
        ));
        all_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Project,
            full_w,
        ));
        all_lines.extend(build_category_from_commands(
            crate::app::actions::ActionCategory::Application,
            full_w,
        ));
        frame.render_widget(Paragraph::new(all_lines), chunks[0]);
    }

    let mouse_line = Line::from(vec![
        Span::styled("Mouse: ", theme.help_category),
        Span::styled(
            "Click: Select/Focus  •  DblClick: Open/Preview  •  Wheel: Scroll",
            theme.dialog_hint,
        ),
    ]);
    frame.render_widget(
        Paragraph::new(mouse_line).alignment(Alignment::Center),
        chunks[1],
    );

    let footer_line = Line::from(Span::styled(
        "Press Esc, Enter, or 'q' to close",
        theme.dialog_hint,
    ));
    frame.render_widget(
        Paragraph::new(footer_line).alignment(Alignment::Center),
        chunks[2],
    );
}

/// Renders the bookmarks list dialog.
fn render_bookmarks_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = centered_rect(64, 14, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = Theme::default();
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Bookmarks ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.dialog_border);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let bookmarks_state = app.bookmarks();
    let bookmarks = bookmarks_state.bookmarks();
    let selected = bookmarks_state.selected_index();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Bookmarks list
            Constraint::Length(1), // Footer hint
        ])
        .split(inner);

    let list_height = chunks[0].height as usize;
    let list_width = chunks[0].width as usize;
    let mut lines = Vec::new();

    if bookmarks.is_empty() {
        lines.push(Line::from(Span::styled(
            "   No bookmarks saved. Press 'b' to bookmark a directory.",
            theme.empty_state_text,
        )));
    } else {
        let scroll_offset = if selected >= list_height {
            selected.saturating_sub(list_height - 1)
        } else {
            0
        };

        let name_col_w = (16).min(list_width.saturating_sub(6).max(6));
        let max_path_width = list_width.saturating_sub(name_col_w + 6);

        for (idx, bookmark) in bookmarks
            .iter()
            .skip(scroll_offset)
            .take(list_height)
            .enumerate()
        {
            let actual_idx = scroll_offset + idx;
            let is_selected = actual_idx == selected;

            let path_str = bookmark.path().display().to_string();
            let truncated_path = truncate_to_width(&path_str, max_path_width);
            let truncated_name = truncate_to_width(bookmark.name(), name_col_w);
            let formatted_name = format!("{:<width$}", truncated_name, width = name_col_w);

            let line = if is_selected {
                Line::from(vec![
                    Span::styled(" ▶ ", theme.dialog_border),
                    Span::styled(formatted_name, theme.dialog_button_active),
                    Span::raw("  "),
                    Span::styled(truncated_path, theme.dialog_message),
                ])
            } else {
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(formatted_name, theme.dialog_border),
                    Span::raw("  "),
                    Span::styled(truncated_path, theme.dialog_hint),
                ])
            };
            lines.push(line);
        }
    }

    frame.render_widget(Paragraph::new(lines), chunks[0]);

    let footer_line = Line::from(Span::styled(
        "↑/↓: Navigate  •  Enter: Open  •  d: Remove  •  Esc: Close",
        theme.dialog_hint,
    ));
    frame.render_widget(
        Paragraph::new(footer_line).alignment(Alignment::Center),
        chunks[1],
    );
}

/// Renders the Smart Jump popup dialog.
fn render_smart_jump_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = centered_rect(68, 18, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = Theme::default();
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Smart Jump (Shift+J) ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.palette_shortcut);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let smart_jump = app.smart_jump();
    let query = smart_jump.query();
    let selected = smart_jump.selected_index();
    let items = smart_jump.filtered_items();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Query input box
            Constraint::Length(1), // Separator
            Constraint::Min(1),    // Results list
            Constraint::Length(1), // Footer hint
        ])
        .split(inner);

    // 1. Query input box
    let query_line = Line::from(vec![
        Span::styled(" > ", theme.palette_shortcut),
        Span::styled(query, theme.palette_input),
        Span::styled("█", theme.palette_shortcut),
    ]);
    frame.render_widget(Paragraph::new(query_line), chunks[0]);

    // 2. Separator line
    let sep = "─".repeat(chunks[1].width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(sep, theme.dialog_hint)),
        chunks[1],
    );

    // 3. Jump target list
    let list_height = chunks[2].height as usize;
    let list_width = chunks[2].width as usize;
    let mut jump_lines = Vec::new();

    if items.is_empty() {
        jump_lines.push(Line::from(Span::styled(
            "   No matching jump locations",
            theme.empty_state_text,
        )));
    } else {
        let scroll_offset = if selected >= list_height {
            selected.saturating_sub(list_height - 1)
        } else {
            0
        };

        let badge_width = 8;
        let title_col_width = (18).min(list_width.saturating_sub(badge_width + 8).max(6));
        let max_path_width = list_width.saturating_sub(badge_width + title_col_width + 6);

        for (idx, item) in items
            .iter()
            .skip(scroll_offset)
            .take(list_height)
            .enumerate()
        {
            let actual_idx = scroll_offset + idx;
            let is_selected = actual_idx == selected;

            let badge = format!("[{}]", item.category);
            let formatted_badge = format!("{:<10}", badge);
            let truncated_title = truncate_to_width(&item.title, title_col_width);
            let formatted_title = format!("{:<width$}", truncated_title, width = title_col_width);
            let path_str = item.path.display().to_string();
            let truncated_path = truncate_to_width(&path_str, max_path_width);

            let mut spans = Vec::new();
            if is_selected {
                spans.push(Span::styled(" ▶ ", theme.palette_shortcut));
                spans.push(Span::styled(formatted_badge, theme.palette_shortcut));
                spans.push(Span::styled(formatted_title, theme.palette_selected));
                if max_path_width > 0 {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(truncated_path, theme.palette_description));
                }
            } else {
                spans.push(Span::raw("   "));
                spans.push(Span::styled(formatted_badge, theme.dialog_hint));
                spans.push(Span::styled(formatted_title, theme.palette_unselected));
                if max_path_width > 0 {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(truncated_path, theme.dialog_hint));
                }
            }
            jump_lines.push(Line::from(spans));
        }
    }

    frame.render_widget(Paragraph::new(jump_lines), chunks[2]);

    // 4. Footer
    let hint = Line::from(Span::styled(
        "↑/↓: Navigate  •  Enter: Jump  •  Esc: Cancel",
        theme.dialog_hint,
    ));
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), chunks[3]);
}

/// Renders the compact Project Cockpit modal dialog.
fn render_project_cockpit_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = centered_rect(68, 20, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = Theme::default();
    frame.render_widget(Clear, dialog_area);

    let active_pane = app.pane(app.active_pane());
    let project_info = active_pane.project_info();
    let git_status = active_pane.git_status();

    let block = Block::default()
        .title(" ⚡ Project Cockpit ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.dialog_border);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Project Info Header
            Constraint::Length(1), // Separator
            Constraint::Min(4),    // Actions List
            Constraint::Length(1), // Footer
        ])
        .split(inner);

    // 1. Project Info Header
    let name_str = project_info.name();
    let type_str = project_info
        .primary_type()
        .map(|t| t.display_name())
        .unwrap_or("None");
    let root_str = project_info
        .root
        .as_ref()
        .map(|r| r.display().to_string())
        .unwrap_or_else(|| "No project root detected".to_string());
    let git_desc = if git_status.is_repo() {
        let branch = git_status.branch.display();
        let state = if git_status.is_clean {
            "clean"
        } else {
            "changed"
        };
        format!("{branch} • {state}")
    } else {
        "Not a git repo".to_string()
    };

    let manifest_str = project_info
        .manifest_file
        .as_ref()
        .and_then(|m| m.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("None");

    let info_lines = vec![
        Line::from(vec![
            Span::styled("  Project:  ", theme.dialog_hint),
            Span::styled(name_str, theme.tab_active_focused),
            Span::styled("   Type: ", theme.dialog_hint),
            Span::styled(type_str, theme.palette_shortcut),
            Span::styled("   Git: ", theme.dialog_hint),
            Span::styled(
                git_desc,
                if git_status.is_clean {
                    theme.project_info_clean
                } else {
                    theme.project_info_dirty
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("  Root:     ", theme.dialog_hint),
            Span::styled(
                truncate_to_width(&root_str, (chunks[0].width as usize).saturating_sub(14)),
                theme.palette_unselected,
            ),
        ]),
        Line::from(vec![
            Span::styled("  Manifest: ", theme.dialog_hint),
            Span::styled(manifest_str, theme.palette_shortcut),
            Span::styled("   README: ", theme.dialog_hint),
            Span::styled(
                if project_info.readme_file.is_some() {
                    "Found"
                } else {
                    "None"
                },
                theme.palette_unselected,
            ),
            Span::styled("   LICENSE: ", theme.dialog_hint),
            Span::styled(
                if project_info.license_file.is_some() {
                    "Found"
                } else {
                    "None"
                },
                theme.palette_unselected,
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(info_lines), chunks[0]);

    // 2. Separator
    let sep_text = "─".repeat(chunks[1].width as usize);
    frame.render_widget(Paragraph::new(sep_text).style(theme.dialog_hint), chunks[1]);

    // 3. Actions List
    let cockpit = app.project_cockpit();
    let actions = cockpit.actions();
    let selected = cockpit.selected_index();

    let list_height = chunks[2].height as usize;
    let list_width = chunks[2].width as usize;

    let scroll_offset = if selected >= list_height {
        selected - list_height + 1
    } else {
        0
    };

    let mut action_lines = Vec::new();
    for (idx, action) in actions
        .iter()
        .skip(scroll_offset)
        .take(list_height)
        .enumerate()
    {
        let actual_idx = scroll_offset + idx;
        let is_selected = actual_idx == selected;

        let label_col_width = 24.min(list_width.saturating_sub(10));
        let truncated_label = truncate_to_width(action.label, label_col_width);
        let formatted_label = format!("{:<width$}", truncated_label, width = label_col_width);
        let max_desc_width = list_width.saturating_sub(label_col_width + 8);
        let truncated_desc = truncate_to_width(&action.description, max_desc_width);

        let mut spans = Vec::new();
        if is_selected {
            spans.push(Span::styled(" ▶ ", theme.palette_shortcut));
            spans.push(Span::styled(formatted_label, theme.palette_selected));
            if max_desc_width > 0 {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(truncated_desc, theme.palette_description));
            }
        } else {
            spans.push(Span::raw("   "));
            spans.push(Span::styled(formatted_label, theme.palette_unselected));
            if max_desc_width > 0 {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(truncated_desc, theme.dialog_hint));
            }
        }
        action_lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(action_lines), chunks[2]);

    // 4. Footer
    let hint = Line::from(Span::styled(
        "↑/↓: Select Action  •  Enter: Execute  •  Esc/q: Close",
        theme.dialog_hint,
    ));
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), chunks[3]);
}

/// Renders the Git Status Panel modal dialog.
fn render_git_status_panel_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = centered_rect(64, 18, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = Theme::default();
    frame.render_widget(Clear, dialog_area);

    let active_pane = app.pane(app.active_pane());
    let git_status = active_pane.git_status();

    let block = Block::default()
        .title(" 🌳 Git Status ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.dialog_border);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Summary stats
            Constraint::Length(1), // Separator
            Constraint::Min(4),    // Changed file list
            Constraint::Length(1), // Footer
        ])
        .split(inner);

    // 1. Summary
    let repo_name = git_status
        .repository
        .repo_name()
        .unwrap_or("No Git Repository");
    let branch = git_status.branch.display();
    let stats_str = format!(
        "Modified: {}  •  Added: {}  •  Deleted: {}  •  Untracked: {}",
        git_status.modified_count,
        git_status.added_count,
        git_status.deleted_count,
        git_status.untracked_count
    );

    let summary_lines = vec![
        Line::from(vec![
            Span::styled("  Repo:   ", theme.dialog_hint),
            Span::styled(repo_name, theme.tab_active_focused),
            Span::styled("   Branch: ", theme.dialog_hint),
            Span::styled(branch, theme.palette_shortcut),
        ]),
        Line::from(vec![
            Span::styled("  Status: ", theme.dialog_hint),
            Span::styled(
                stats_str,
                if git_status.is_clean {
                    theme.project_info_clean
                } else {
                    theme.project_info_dirty
                },
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(summary_lines), chunks[0]);

    // 2. Separator
    let sep_text = "─".repeat(chunks[1].width as usize);
    frame.render_widget(Paragraph::new(sep_text).style(theme.dialog_hint), chunks[1]);

    // 3. Changed file list
    let panel = app.git_status_panel();
    let entries = panel.entries();
    let selected = panel.selected_index();

    let list_height = chunks[2].height as usize;
    let list_width = chunks[2].width as usize;

    let scroll_offset = if selected >= list_height {
        selected - list_height + 1
    } else {
        0
    };

    let mut entry_lines = Vec::new();
    if entries.is_empty() {
        entry_lines.push(Line::from(Span::styled(
            "   Working directory is clean (no changes detected)",
            theme.project_info_clean,
        )));
    } else {
        for (idx, entry) in entries
            .iter()
            .skip(scroll_offset)
            .take(list_height)
            .enumerate()
        {
            let actual_idx = scroll_offset + idx;
            let is_selected = actual_idx == selected;

            let code = entry.status.code();
            let code_badge = format!("[{code}]");
            let path_str = entry.relative_path.display().to_string();
            let max_path_width = list_width.saturating_sub(12);
            let truncated_path = truncate_to_width(&path_str, max_path_width);

            let (code_str, code_style) = theme.git_file_status(entry.status);

            let mut spans = Vec::new();
            if is_selected {
                spans.push(Span::styled(" ▶ ", theme.palette_shortcut));
                spans.push(Span::styled(format!("{code_badge:<5}"), code_style));
                spans.push(Span::styled(truncated_path, theme.palette_selected));
            } else {
                spans.push(Span::raw("   "));
                spans.push(Span::styled(format!("{code_badge:<5}"), code_style));
                spans.push(Span::styled(truncated_path, theme.palette_unselected));
            }
            let _ = code_str;
            entry_lines.push(Line::from(spans));
        }
    }
    frame.render_widget(Paragraph::new(entry_lines), chunks[2]);

    // 4. Footer
    let hint = Line::from(Span::styled(
        "↑/↓: Select File  •  Enter: Jump to File  •  Esc/q: Close",
        theme.dialog_hint,
    ));
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), chunks[3]);
}

/// Renders the File Radar directory insight dialog.
fn render_file_radar_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = centered_rect(62, 18, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = Theme::default();
    frame.render_widget(Clear, dialog_area);

    let active_pane = app.pane(app.active_pane());
    let current_dir = active_pane.current_path();
    let radar = app.file_radar();

    let block = Block::default()
        .title(" 📡 File Radar & Directory Insights ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.dialog_border);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Metrics
            Constraint::Length(1), // Separator
            Constraint::Min(4),    // Extension distribution
            Constraint::Length(1), // Footer
        ])
        .split(inner);

    // 1. Directory Metrics
    let path_str = current_dir.display().to_string();
    let size_str = crate::preview::format_size(radar.total_file_bytes);

    let metrics_lines = vec![
        Line::from(vec![
            Span::styled("  Directory:   ", theme.dialog_hint),
            Span::styled(
                truncate_to_width(&path_str, (chunks[0].width as usize).saturating_sub(16)),
                theme.palette_unselected,
            ),
        ]),
        Line::from(vec![
            Span::styled("  Total Items: ", theme.dialog_hint),
            Span::styled(format!("{}", radar.total_entries), theme.palette_shortcut),
            Span::styled("   Files: ", theme.dialog_hint),
            Span::styled(format!("{}", radar.file_count), theme.tab_active_focused),
            Span::styled("   Dirs: ", theme.dialog_hint),
            Span::styled(format!("{}", radar.directory_count), theme.palette_shortcut),
            Span::styled("   Links: ", theme.dialog_hint),
            Span::styled(format!("{}", radar.symlink_count), theme.palette_unselected),
        ]),
        Line::from(vec![
            Span::styled("  File Bytes:  ", theme.dialog_hint),
            Span::styled(size_str, theme.tab_active_focused),
            Span::styled("   Hidden Items: ", theme.dialog_hint),
            Span::styled(format!("{}", radar.hidden_count), theme.dialog_hint),
        ]),
    ];
    frame.render_widget(Paragraph::new(metrics_lines), chunks[0]);

    // 2. Separator
    let sep_text = "─".repeat(chunks[1].width as usize);
    frame.render_widget(Paragraph::new(sep_text).style(theme.dialog_hint), chunks[1]);

    // 3. Extension Distribution
    let mut ext_lines = vec![Line::from(Span::styled(
        "  Top File Types:",
        theme.dialog_hint,
    ))];

    if radar.top_extensions.is_empty() {
        ext_lines.push(Line::from(Span::styled(
            "   No files found in directory",
            theme.dialog_hint,
        )));
    } else {
        for stat in &radar.top_extensions {
            let ext_col = format!("   {:<12}", stat.extension);
            let count_col = format!("{:>4} files", stat.count);
            let bytes_col = format!("{:>12}", crate::preview::format_size(stat.total_bytes));

            ext_lines.push(Line::from(vec![
                Span::styled(ext_col, theme.palette_shortcut),
                Span::styled(count_col, theme.tab_active_focused),
                Span::raw("   "),
                Span::styled(bytes_col, theme.palette_description),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(ext_lines), chunks[2]);

    // 4. Footer
    let hint = Line::from(Span::styled("Esc/q: Close", theme.dialog_hint));
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), chunks[3]);
}

/// Renders the Reveal Context modal dialog showing hierarchical path layers.
fn render_reveal_context_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = centered_rect(66, 16, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = Theme::default();
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" 🔍 Reveal Context ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.dialog_border);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Subtitle
            Constraint::Min(4),    // Context levels list
            Constraint::Length(1), // Footer
        ])
        .split(inner);

    let subtitle = Line::from(Span::styled(
        " Hierarchical context chain for currently selected item:",
        theme.dialog_hint,
    ));
    frame.render_widget(Paragraph::new(subtitle), chunks[0]);

    let ctx = app.reveal_context();
    let levels = ctx.levels();
    let selected = ctx.selected_index();

    let list_height = chunks[1].height as usize;
    let list_width = chunks[1].width as usize;

    let mut level_lines = Vec::new();
    for (idx, level) in levels.iter().enumerate().take(list_height) {
        let is_selected = idx == selected;
        let prefix = if idx == 0 {
            "Item"
        } else if idx == 1 {
            "Dir "
        } else if idx == 2 {
            "Proj"
        } else {
            "Git "
        };

        let badge = format!("[{prefix}]");
        let badge_formatted = format!("{:<7}", badge);
        let truncated_title = truncate_to_width(&level.title, 20);
        let formatted_title = format!("{:<20}", truncated_title);
        let max_detail_width = list_width.saturating_sub(34);
        let truncated_details = truncate_to_width(&level.details, max_detail_width);

        let mut spans = Vec::new();
        if is_selected {
            spans.push(Span::styled(" ▶ ", theme.palette_shortcut));
            spans.push(Span::styled(badge_formatted, theme.palette_shortcut));
            spans.push(Span::styled(formatted_title, theme.palette_selected));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(truncated_details, theme.palette_description));
        } else {
            spans.push(Span::raw("   "));
            spans.push(Span::styled(badge_formatted, theme.dialog_hint));
            spans.push(Span::styled(formatted_title, theme.palette_unselected));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(truncated_details, theme.dialog_hint));
        }
        level_lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(level_lines), chunks[1]);

    let hint = Line::from(Span::styled(
        "↑/↓: Select Level  •  Enter: Jump to Level  •  Esc/q: Close",
        theme.dialog_hint,
    ));
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn centered_rect_calculates_bounds_correctly() {
        let area = Rect::new(0, 0, 100, 50);
        let popup = centered_rect(50, 20, area);

        assert_eq!(popup.width, 50);
        assert_eq!(popup.height, 20);
        assert_eq!(popup.x, 25);
        assert_eq!(popup.y, 15);
    }

    #[test]
    fn centered_rect_clamps_to_small_areas() {
        let area = Rect::new(0, 0, 30, 10);
        let popup = centered_rect(60, 20, area);

        assert!(popup.width <= 30);
        assert!(popup.height <= 10);
        assert_eq!(popup.x, 1);
        assert_eq!(popup.y, 1);
    }

    #[test]
    fn centered_rect_handles_zero_area_safely() {
        let zero = Rect::new(0, 0, 0, 0);
        let popup = centered_rect(50, 20, zero);
        assert_eq!(popup, Rect::default());
    }

    #[test]
    fn render_confirm_dialog_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        app.handle_action(crate::app::actions::Action::NewFile); // modal mode

        terminal
            .draw(|f| {
                render_confirm_dialog(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..24)
            .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("Confirm Action"));
        assert!(text.contains("Yes"));
        assert!(text.contains("No"));
    }

    #[test]
    fn render_input_dialog_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        app.handle_action(crate::app::actions::Action::NewFile);

        terminal
            .draw(|f| {
                render_input_dialog(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..24)
            .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("New File"));
    }

    #[test]
    fn render_command_palette_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        app.handle_action(crate::app::actions::Action::CommandPalette);

        terminal
            .draw(|f| {
                render_command_palette(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..24)
            .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("Command Palette"));
    }

    #[test]
    fn render_help_dialog_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();

        terminal
            .draw(|f| {
                render_help_dialog(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..24)
            .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("Help & Keyboard Shortcuts"));
        assert!(text.contains("Navigation"));
    }

    #[test]
    fn render_bookmarks_dialog_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();

        terminal
            .draw(|f| {
                render_bookmarks_dialog(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..24)
            .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("Bookmarks"));
    }

    #[test]
    fn render_smart_jump_dialog_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        app.handle_action(crate::app::actions::Action::SmartJump);

        terminal
            .draw(|f| {
                render_smart_jump_dialog(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..24)
            .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("Smart Jump"));
    }

    #[test]
    fn render_project_cockpit_dialog_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        app.handle_action(crate::app::actions::Action::ProjectCockpit);

        terminal
            .draw(|f| {
                render_project_cockpit_dialog(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..24)
            .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("Project Cockpit"));
    }

    #[test]
    fn render_git_status_panel_dialog_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        app.handle_action(crate::app::actions::Action::GitStatusPanel);

        terminal
            .draw(|f| {
                render_git_status_panel_dialog(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..24)
            .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("Git Status"));
    }

    #[test]
    fn render_file_radar_dialog_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        app.handle_action(crate::app::actions::Action::FileRadar);

        terminal
            .draw(|f| {
                render_file_radar_dialog(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..24)
            .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("File Radar"));
    }

    #[test]
    fn render_reveal_context_dialog_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        app.handle_action(crate::app::actions::Action::RevealContext);

        terminal
            .draw(|f| {
                render_reveal_context_dialog(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..24)
            .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("Reveal Context"));
    }

    #[test]
    fn dialogs_tiny_terminal_does_not_panic() {
        let tiny_sizes = [(1, 1), (2, 2), (5, 5), (10, 3)];
        let mut app = App::default();
        app.set_mode(Mode::Confirm);

        for (w, h) in tiny_sizes {
            let backend = TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|f| {
                    render(f, f.area(), &app);
                })
                .unwrap();
        }
    }

    #[test]
    fn test_all_dialogs_across_required_dimensions() {
        let required_sizes = [
            (40, 10),
            (60, 15),
            (80, 24),
            (100, 30),
            (120, 30),
            (160, 40),
            (200, 60),
            (240, 80),
        ];

        let modes = [
            Mode::Confirm,
            Mode::Create,
            Mode::Rename,
            Mode::CommandPalette,
            Mode::Help,
            Mode::Bookmarks,
            Mode::Jump,
            Mode::SmartJump,
            Mode::ProjectCockpit,
            Mode::GitStatusPanel,
            Mode::FileRadar,
            Mode::RevealContext,
        ];

        for (cols, rows) in required_sizes {
            let backend = TestBackend::new(cols, rows);
            let mut terminal = Terminal::new(backend).unwrap();

            for mode in modes {
                let mut app = App::default();
                app.set_mode(mode);

                terminal
                    .draw(|f| {
                        render(f, f.area(), &app);
                    })
                    .expect("dialog render must succeed at all required sizes");

                // Verify buffer does not crash and lines stay in bounds
                let buffer = terminal.backend().buffer();
                assert_eq!(buffer.area.width, cols);
                assert_eq!(buffer.area.height, rows);
            }
        }
    }

    #[test]
    fn test_stage7_ultimate_responsive_audit_all_dimensions_and_odd_sizes() {
        let all_sizes = [
            (1, 1),
            (20, 5),
            (40, 10),
            (60, 15),
            (73, 17),
            (80, 24),
            (97, 23),
            (100, 30),
            (113, 29),
            (120, 30),
            (137, 37),
            (140, 40),
            (160, 40),
            (167, 43),
            (180, 50),
            (200, 60),
            (203, 61),
            (240, 80),
            (300, 100),
            (400, 120),
        ];

        for (w, h) in all_sizes {
            let backend = TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).unwrap();

            // Test normal mode
            let mut app = App::default();
            terminal
                .draw(|f| {
                    crate::ui::render(f, &app);
                })
                .unwrap();

            // Test focus mode
            app.toggle_focus_mode();
            assert!(app.is_focus_mode());
            terminal
                .draw(|f| {
                    crate::ui::render(f, &app);
                })
                .unwrap();

            // Test Help dialog overlay
            app.set_mode(Mode::Help);
            terminal
                .draw(|f| {
                    crate::ui::render(f, &app);
                })
                .unwrap();
        }
    }
}
