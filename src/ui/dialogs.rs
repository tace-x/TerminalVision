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
use crate::app::state::{App, ContextMenuItem, CreateKind};
use crate::ui::theme::{Theme, ThemeId};
use crate::ui::{display_width, truncate_to_width};

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

/// Computes an animated centered rectangle for modal dialogs based on active animation progress.
pub fn animated_dialog_rect(
    max_width: u16,
    max_height: u16,
    area: Rect,
    app: &App,
    tag: crate::animation::AnimationTag,
) -> Rect {
    let target = centered_rect(max_width, max_height, area);
    if let Some(progress) = app
        .animation_engine()
        .tag_progress(tag)
        .filter(|p| p.state.is_active() && app.motion_mode() == crate::animation::MotionMode::Full)
    {
        let t = 0.88 + (progress.eased * 0.12);
        let cur_w = ((target.width as f32) * t).round() as u16;
        let cur_h = ((target.height as f32) * t).round() as u16;
        let center_x = target.x + target.width / 2;
        let center_y = target.y + target.height / 2;
        let x = center_x.saturating_sub(cur_w / 2);
        let y = center_y.saturating_sub(cur_h / 2);
        return crate::animation::clamp_rect_to_bounds(
            Rect::new(x, y, cur_w.max(1), cur_h.max(1)),
            area,
        );
    }
    target
}

/// Computes the centered bounding rectangle for the Theme Selector modal.
pub fn calculate_theme_selector_rect(area: Rect) -> Rect {
    centered_rect(70, 16, area)
}

/// Renders the active modal dialog or command palette on top of the existing interface.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.width < 4 || area.height < 4 {
        return;
    }

    let theme = app.theme();

    // 1. Conflict resolution prompt takes top priority
    if let Some(conflict) = app.operation_manager().active_conflict.as_ref() {
        render_conflict_dialog(frame, area, conflict, &theme);
        return;
    }

    // 2. Error recovery dialog
    if let Some(err_prompt) = app.operation_manager().active_error.as_ref() {
        render_error_recovery_dialog(frame, area, err_prompt, &theme);
        return;
    }

    // 3. Running Operation Center progress dialog
    if let Some(metrics) = app.operation_manager().active_metrics.as_ref() {
        render_operation_progress_dialog(frame, area, metrics, &theme);
        return;
    }

    // 4. Modal mode overlays
    match app.mode() {
        Mode::Preview => render_quick_preview_dialog(frame, area, app),
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
        Mode::ContextMenu => render_context_menu_dialog(frame, area, app),
        Mode::StorageVision => render_storage_vision_dialog(frame, area, app),
        Mode::ThemeSelector => render_theme_selector_dialog(frame, area, app),
        _ => {}
    }
}

/// Renders a confirmation prompt dialog (e.g. Delete confirmation).
fn render_confirm_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area =
        animated_dialog_rect(54, 10, area, app, crate::animation::AnimationTag::Dialog);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = app.theme();
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
    let dialog_area =
        animated_dialog_rect(56, 8, area, app, crate::animation::AnimationTag::Dialog);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = app.theme();
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
    let dialog_area = animated_dialog_rect(
        64,
        7,
        area,
        app,
        crate::animation::AnimationTag::QuickSwitcher,
    );
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = app.theme();
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

/// Calculates the dialog area Rect for the Command Center.
pub fn calculate_command_palette_rect(area: Rect) -> Rect {
    centered_rect(66, 18, area)
}

/// Calculates the dialog area Rect for the Quick Switcher.
pub fn calculate_smart_jump_rect(area: Rect) -> Rect {
    centered_rect(68, 18, area)
}

/// Renders the searchable Command Center overlay.
fn render_command_palette(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = animated_dialog_rect(
        66,
        18,
        area,
        app,
        crate::animation::AnimationTag::CommandCenter,
    );
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = app.theme();
    frame.render_widget(Clear, dialog_area);

    let title = if crate::input::platform::Platform::current().is_mac() {
        " Command Center (⌘K) "
    } else {
        " Command Center (Ctrl+K) "
    };

    let block = Block::default()
        .title(title)
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
    let entries = palette.entries();

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
    let prompt = if query.is_empty() {
        Span::styled(" 🔍 Search TerminalVision...", theme.dialog_hint)
    } else {
        Span::styled(query, theme.palette_input)
    };

    let query_line = Line::from(vec![
        Span::styled(" 🔍 ", theme.palette_shortcut),
        prompt,
        if !query.is_empty() {
            Span::styled("█", theme.palette_shortcut)
        } else {
            Span::raw("")
        },
    ]);
    frame.render_widget(Paragraph::new(query_line), chunks[0]);

    // 2. Separator line
    let sep = "─".repeat(chunks[1].width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(sep, theme.dialog_hint)),
        chunks[1],
    );

    // 3. Results list
    let list_height = chunks[2].height as usize;
    let list_width = chunks[2].width as usize;
    let mut lines = Vec::new();

    if entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "   No matching actions or files",
            theme.empty_state_text,
        )));
    } else {
        let scroll_offset = if selected >= list_height {
            selected.saturating_sub(list_height - 1)
        } else {
            0
        };

        let badge_width = 8;
        let name_col_width = (20).min(list_width.saturating_sub(badge_width + 8).max(6));
        let avail_desc = list_width.saturating_sub(badge_width + name_col_width + 12);
        let platform = crate::input::platform::Platform::current();

        for (idx, entry) in entries
            .iter()
            .skip(scroll_offset)
            .take(list_height)
            .enumerate()
        {
            let actual_idx = scroll_offset + idx;
            let is_selected = actual_idx == selected;

            let badge = entry.category_badge();
            let formatted_badge = format!("{:<8}", badge);
            let truncated_name = truncate_to_width(&entry.title(), name_col_width);
            let formatted_name = format!("{:<width$}", truncated_name, width = name_col_width);

            let mut spans = Vec::new();
            if is_selected {
                spans.push(Span::styled(" ▶ ", theme.palette_shortcut));
                spans.push(Span::styled(formatted_badge, theme.palette_shortcut));
                spans.push(Span::styled(formatted_name, theme.palette_selected));
                if avail_desc > 0 {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        truncate_to_width(&entry.description(), avail_desc),
                        theme.palette_description,
                    ));
                }
                if let Some(sc) = entry.shortcut(platform) {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(format!("[{sc}]"), theme.palette_shortcut));
                }
            } else {
                spans.push(Span::raw("   "));
                spans.push(Span::styled(formatted_badge, theme.dialog_hint));
                spans.push(Span::styled(formatted_name, theme.palette_unselected));
                if avail_desc > 0 {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        truncate_to_width(&entry.description(), avail_desc),
                        theme.dialog_hint,
                    ));
                }
                if let Some(sc) = entry.shortcut(platform) {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(format!("[{sc}]"), theme.dialog_hint));
                }
            }
            lines.push(Line::from(spans));
        }
    }

    frame.render_widget(Paragraph::new(lines), chunks[2]);

    // 4. Footer
    let hint = Line::from(Span::styled(
        "↑/↓: Navigate  •  Enter: Execute  •  Esc: Close",
        theme.dialog_hint,
    ));
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), chunks[3]);
}

/// Renders the help / keyboard shortcuts reference dialog.
fn render_help_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area =
        animated_dialog_rect(80, 22, area, app, crate::animation::AnimationTag::Dialog);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = app.theme();
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

    let build_category_from_registry =
        |cat: crate::app::actions::ActionCategory, max_w: usize| -> Vec<Line<'static>> {
            let mut lines = Vec::new();
            lines.push(Line::from(Span::styled(
                format!("── {} ──", cat.display_name()),
                theme.help_category,
            )));
            let key_col_w = (14).min(max_w.saturating_sub(4).max(4));
            let avail_desc = max_w.saturating_sub(key_col_w + 3);
            let shortcuts = crate::input::shortcut::ShortcutRegistry::global()
                .shortcuts_by_category(cat, crate::input::platform::Platform::current());
            for (_action, sc, desc) in shortcuts {
                let key_str = format!(
                    "  {:<width$}",
                    truncate_to_width(&sc, key_col_w),
                    width = key_col_w
                );
                let desc_str = truncate_to_width(desc, avail_desc);
                lines.push(Line::from(vec![
                    Span::styled(key_str, theme.help_key),
                    Span::styled(desc_str, theme.help_desc),
                ]));
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
            build_category_from_registry(crate::app::actions::ActionCategory::Navigation, half_w);
        left_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Files,
            half_w,
        ));
        left_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Tabs,
            half_w,
        ));
        left_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Bookmarks,
            half_w,
        ));
        left_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Search,
            half_w,
        ));

        let mut right_lines =
            build_category_from_registry(crate::app::actions::ActionCategory::View, half_w);
        right_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Preview,
            half_w,
        ));
        right_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Git,
            half_w,
        ));
        right_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Project,
            half_w,
        ));
        right_lines.extend(build_category_from_registry(
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
            build_category_from_registry(crate::app::actions::ActionCategory::Navigation, full_w);
        all_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Files,
            full_w,
        ));
        all_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Tabs,
            full_w,
        ));
        all_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Bookmarks,
            full_w,
        ));
        all_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Search,
            full_w,
        ));
        all_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::View,
            full_w,
        ));
        all_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Preview,
            full_w,
        ));
        all_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Git,
            full_w,
        ));
        all_lines.extend(build_category_from_registry(
            crate::app::actions::ActionCategory::Project,
            full_w,
        ));
        all_lines.extend(build_category_from_registry(
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

/// Renders the favorites / quick access list dialog.
fn render_bookmarks_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area =
        animated_dialog_rect(66, 15, area, app, crate::animation::AnimationTag::Dialog);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = app.theme();
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" ★ Favorites / Bookmarks ")
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
            "   No favorites yet. Add a folder from the Command Center (⌘K) or press [A].",
            theme.empty_state_text,
        )));
    } else {
        let scroll_offset = if selected >= list_height {
            selected.saturating_sub(list_height - 1)
        } else {
            0
        };

        let name_col_w = (18).min(list_width.saturating_sub(10).max(6));
        let max_path_width = list_width.saturating_sub(name_col_w + 10);

        for (idx, bookmark) in bookmarks
            .iter()
            .skip(scroll_offset)
            .take(list_height)
            .enumerate()
        {
            let actual_idx = scroll_offset + idx;
            let is_selected = actual_idx == selected;
            let is_valid = bookmark.is_valid();

            let icon_span = if !is_valid {
                Span::styled("⚠ ", theme.project_info_dirty)
            } else {
                Span::styled("★ ", theme.project_info_clean)
            };

            let path_str = bookmark.path().display().to_string();
            let truncated_path = truncate_to_width(&path_str, max_path_width);
            let truncated_name = truncate_to_width(bookmark.name(), name_col_w);
            let formatted_name = format!("{:<width$}", truncated_name, width = name_col_w);

            let line = if is_selected {
                Line::from(vec![
                    Span::styled(" ▶ ", theme.dialog_border),
                    icon_span,
                    Span::styled(formatted_name, theme.dialog_button_active),
                    Span::raw("  "),
                    Span::styled(truncated_path, theme.dialog_message),
                ])
            } else {
                Line::from(vec![
                    Span::raw("   "),
                    icon_span,
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
        "↑/↓: Navigate  •  Enter: Open  •  K/J: Move  •  d: Remove  •  Esc: Close",
        theme.dialog_hint,
    ));
    frame.render_widget(
        Paragraph::new(footer_line).alignment(Alignment::Center),
        chunks[1],
    );
}

/// Renders the Storage Vision directory analysis and heatmap workspace.
fn render_storage_vision_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area =
        animated_dialog_rect(84, 24, area, app, crate::animation::AnimationTag::Dialog);
    if dialog_area.width < 10 || dialog_area.height < 6 {
        return;
    }

    let theme = app.theme();
    frame.render_widget(Clear, dialog_area);

    let storage_state = app.storage_vision();
    let is_scanning = storage_state.is_scanning;
    let live_items = storage_state.live_items_count();
    let live_bytes = storage_state.live_bytes_count();

    let title = if is_scanning {
        format!(
            " Storage Vision [SCANNING... {} items • {}] ",
            live_items,
            crate::storage::format_storage_size(live_bytes)
        )
    } else {
        " Storage Vision ".to_string()
    };

    let block = Block::default()
        .title(title)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if is_scanning {
            theme.project_info_dirty
        } else {
            theme.dialog_border
        });

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height < 4 || inner.width < 10 {
        return;
    }

    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Scope & summary
            Constraint::Min(1),    // Main content
            Constraint::Length(1), // Footer hint
        ])
        .split(inner);

    let scope_path_str = storage_state.current_scope.display().to_string();
    let total_size_str = if let Some(ref analysis) = storage_state.analysis {
        analysis.formatted_total_size()
    } else {
        crate::storage::format_storage_size(live_bytes)
    };

    let header_line1 = Line::from(vec![
        Span::styled("Scope: ", theme.dialog_button_active),
        Span::styled(scope_path_str, theme.dialog_border),
        Span::styled("   Total: ", theme.dialog_button_active),
        Span::styled(total_size_str, theme.project_info_clean),
    ]);

    let skipped_info = if let Some(ref analysis) = storage_state.analysis {
        if analysis.skipped_permissions > 0 {
            format!(
                "  (Skipped {} inaccessible dirs)",
                analysis.skipped_permissions
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let history_depth = storage_state.history_stack.len();
    let history_str = if history_depth > 0 {
        format!("Depth: {} (Backspace to go back)", history_depth)
    } else {
        "Root Scope".to_string()
    };

    let header_line2 = Line::from(vec![
        Span::styled(history_str, theme.dialog_hint),
        Span::styled(skipped_info, theme.project_info_dirty),
    ]);

    frame.render_widget(
        Paragraph::new(vec![header_line1, header_line2]),
        vertical_chunks[0],
    );

    let is_wide = vertical_chunks[1].width >= 64;
    if is_wide {
        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(45), // Category Heatmap
                Constraint::Percentage(55), // Children & Largest Files
            ])
            .split(vertical_chunks[1]);

        render_storage_category_panel(frame, horizontal_chunks[0], storage_state, &theme);
        render_storage_children_panel(frame, horizontal_chunks[1], storage_state, &theme);
    } else {
        render_storage_children_panel(frame, vertical_chunks[1], storage_state, &theme);
    }

    let footer_line = Line::from(Span::styled(
        "Enter: Drill Down  •  Backspace/h: Back  •  o: Open in Pane  •  Esc: Close",
        theme.dialog_hint,
    ));
    frame.render_widget(
        Paragraph::new(footer_line).alignment(Alignment::Center),
        vertical_chunks[2],
    );
}

/// Renders the category heatmap breakdown panel.
fn render_storage_category_panel(
    frame: &mut Frame,
    area: Rect,
    state: &crate::storage::StorageVisionState,
    theme: &Theme,
) {
    let block = Block::default()
        .title(" Category Breakdown ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.dialog_border);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let mut lines = Vec::new();
    if let Some(ref analysis) = state.analysis {
        let total_b = analysis.total_bytes.max(1) as f32;
        let bar_width = (inner.width as usize).saturating_sub(22).max(4);

        for (cat, bytes, _count) in analysis
            .category_breakdown
            .iter()
            .take(inner.height as usize)
        {
            let pct = (*bytes as f32 / total_b) * 100.0;
            let bar_str = crate::storage::render_storage_bar(pct, bar_width);
            let size_str = crate::storage::format_storage_size(*bytes);
            let name_str = format!("{:<10}", format!("{:?}", cat));

            lines.push(Line::from(vec![
                Span::styled(name_str, theme.dialog_border),
                Span::styled(format!("{size_str:>8} "), theme.project_info_clean),
                Span::styled(bar_str, theme.tab_active_focused),
                Span::styled(format!(" {pct:>4.1}%"), theme.dialog_hint),
            ]));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Analyzing categories...",
            theme.empty_state_text,
        )));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Renders the subdirectories drill-down list and top largest files.
fn render_storage_children_panel(
    frame: &mut Frame,
    area: Rect,
    state: &crate::storage::StorageVisionState,
    theme: &Theme,
) {
    let block = Block::default()
        .title(" Subdirectories & Items ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.dialog_border);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let mut lines = Vec::new();
    if let Some(ref analysis) = state.analysis {
        let children = &analysis.direct_children;
        if children.is_empty() {
            lines.push(Line::from(Span::styled(
                "   (No subdirectories or files in this scope)",
                theme.empty_state_text,
            )));
        } else {
            let max_rows = inner.height as usize;
            let selected = state.selected_index;
            let scroll_offset = if selected >= max_rows {
                selected.saturating_sub(max_rows - 1)
            } else {
                0
            };

            let name_width = (inner.width as usize).saturating_sub(26).max(8);
            let bar_width = 8;

            for (idx, child) in children
                .iter()
                .skip(scroll_offset)
                .take(max_rows)
                .enumerate()
            {
                let actual_idx = scroll_offset + idx;
                let is_selected = actual_idx == selected;

                let dir_suffix = if child.is_dir { "/" } else { "" };
                let full_name = format!("{}{dir_suffix}", child.name);
                let truncated_name = truncate_to_width(&full_name, name_width);
                let formatted_name = format!("{:<width$}", truncated_name, width = name_width);
                let size_str = crate::storage::format_storage_size(child.bytes);
                let bar_str = crate::storage::render_storage_bar(child.percentage, bar_width);

                let line = if is_selected {
                    Line::from(vec![
                        Span::styled(" ▶ ", theme.dialog_border),
                        Span::styled(formatted_name, theme.dialog_button_active),
                        Span::styled(format!("{size_str:>8} "), theme.project_info_clean),
                        Span::styled(bar_str, theme.tab_active_focused),
                        Span::styled(format!(" {:>3.0}%", child.percentage), theme.dialog_hint),
                    ])
                } else {
                    Line::from(vec![
                        Span::raw("   "),
                        Span::styled(
                            formatted_name,
                            if child.is_dir {
                                theme.dialog_message
                            } else {
                                theme.dialog_hint
                            },
                        ),
                        Span::styled(format!("{size_str:>8} "), theme.project_info_clean),
                        Span::styled(bar_str, theme.dialog_border),
                        Span::styled(format!(" {:>3.0}%", child.percentage), theme.dialog_hint),
                    ])
                };

                lines.push(line);
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Scanning directory hierarchy...",
            theme.empty_state_text,
        )));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Renders the Quick Switcher popup dialog.
fn render_smart_jump_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = animated_dialog_rect(
        68,
        18,
        area,
        app,
        crate::animation::AnimationTag::QuickSwitcher,
    );
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = app.theme();
    frame.render_widget(Clear, dialog_area);

    let title = if crate::input::platform::Platform::current().is_mac() {
        " Quick Switcher (⌘P) "
    } else {
        " Quick Switcher (Ctrl+P) "
    };

    let block = Block::default()
        .title(title)
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
    let prompt = if query.is_empty() {
        Span::styled(" 🔍 Jump to...", theme.dialog_hint)
    } else {
        Span::styled(query, theme.palette_input)
    };

    let query_line = Line::from(vec![
        Span::styled(" 🔍 ", theme.palette_shortcut),
        prompt,
        if !query.is_empty() {
            Span::styled("█", theme.palette_shortcut)
        } else {
            Span::raw("")
        },
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
            "   No matching locations or files",
            theme.empty_state_text,
        )));
    } else {
        let scroll_offset = if selected >= list_height {
            selected.saturating_sub(list_height - 1)
        } else {
            0
        };

        let badge_width = 8;
        let title_col_width = (22).min(list_width.saturating_sub(badge_width + 8).max(6));
        let max_path_width = list_width.saturating_sub(badge_width + title_col_width + 6);

        for (idx, item) in items
            .iter()
            .skip(scroll_offset)
            .take(list_height)
            .enumerate()
        {
            let actual_idx = scroll_offset + idx;
            let is_selected = actual_idx == selected;

            let icon = if item.is_file { "📄 " } else { "📁 " };
            let badge = format!("[{}]", item.category);
            let formatted_badge = format!("{:<9}", badge);
            let truncated_title = truncate_to_width(&item.title, title_col_width.saturating_sub(2));
            let title_with_icon = format!("{}{}", icon, truncated_title);
            let formatted_title = format!("{:<width$}", title_with_icon, width = title_col_width);
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
    let dialog_area =
        animated_dialog_rect(70, 22, area, app, crate::animation::AnimationTag::Dialog);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = app.theme();
    frame.render_widget(Clear, dialog_area);

    let active_pane = app.pane(app.active_pane());
    let current_path = active_pane.current_path();
    let ws = active_pane.workspace_context();
    let proj = ws.project_for_path(current_path);
    let project_info = active_pane.project_info();
    let git_status = active_pane.git_status();

    let block = Block::default()
        .title(" ⚡ Project Overview ")
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
            Constraint::Length(5), // Project Info Header
            Constraint::Length(1), // Separator
            Constraint::Min(4),    // Actions List
            Constraint::Length(1), // Footer
        ])
        .split(inner);

    // 1. Project Info Header
    let name_str = if let Some(p) = proj {
        p.name.as_str()
    } else {
        project_info.name()
    };

    let type_str = if let Some(p) = proj {
        if !p.languages.is_empty() {
            p.languages
                .iter()
                .map(|l| l.display_name())
                .collect::<Vec<_>>()
                .join(" · ")
        } else if p.project_type != crate::project::ProjectType::Generic {
            p.project_type.display_name().to_string()
        } else {
            "Generic".to_string()
        }
    } else {
        project_info
            .primary_type()
            .map(|t| t.display_name().to_string())
            .unwrap_or_else(|| "Generic".to_string())
    };

    let root_str = if let Some(p) = proj {
        p.root.display().to_string()
    } else if let Some(ref r) = ws.root {
        r.display().to_string()
    } else {
        project_info
            .root
            .as_ref()
            .map(|r| r.display().to_string())
            .unwrap_or_else(|| "No project root detected".to_string())
    };

    let git_desc = if git_status.is_repo() {
        let branch = git_status.branch.display();
        let state = if git_status.is_clean {
            "clean".to_string()
        } else {
            let mut parts = Vec::new();
            if git_status.added_count > 0 {
                parts.push(format!("+{}", git_status.added_count));
            }
            if git_status.modified_count > 0 {
                parts.push(format!("~{}", git_status.modified_count));
            }
            if git_status.deleted_count > 0 {
                parts.push(format!("-{}", git_status.deleted_count));
            }
            if git_status.untracked_count > 0 {
                parts.push(format!("?{}", git_status.untracked_count));
            }
            if parts.is_empty() {
                "modified".to_string()
            } else {
                parts.join(" ")
            }
        };
        format!("{branch} • {state}")
    } else {
        "Not a git repo".to_string()
    };

    let manifest_str = if let Some(p) = proj {
        p.primary_manifest()
            .map(|m| m.name.as_str())
            .unwrap_or("None")
    } else {
        project_info
            .manifest_file
            .as_ref()
            .and_then(|m| m.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("None")
    };

    let source_str = if let Some(p) = proj {
        p.source_directories
            .first()
            .map(|s| s.name.as_str())
            .unwrap_or("None")
    } else {
        project_info
            .source_dir
            .as_ref()
            .and_then(|s| s.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("None")
    };

    let tests_str = if let Some(p) = proj {
        p.test_directories
            .first()
            .map(|t| t.name.as_str())
            .unwrap_or("None")
    } else {
        "None"
    };

    let docs_str = if let Some(p) = proj {
        p.documentation_directories
            .first()
            .map(|d| d.name.as_str())
            .unwrap_or("None")
    } else {
        "None"
    };

    let mut info_lines = vec![
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
            Span::styled("   Source: ", theme.dialog_hint),
            Span::styled(source_str, theme.palette_unselected),
            Span::styled("   Tests: ", theme.dialog_hint),
            Span::styled(tests_str, theme.palette_unselected),
            Span::styled("   Docs: ", theme.dialog_hint),
            Span::styled(docs_str, theme.palette_unselected),
        ]),
    ];

    if ws.is_monorepo {
        info_lines.push(Line::from(vec![
            Span::styled("  Workspace: ", theme.dialog_hint),
            Span::styled(ws.name(), theme.tab_active_focused),
            Span::styled(
                format!(" ({} projects detected)", ws.projects.len()),
                theme.palette_description,
            ),
        ]));
    }

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
    let dialog_area =
        animated_dialog_rect(64, 18, area, app, crate::animation::AnimationTag::Dialog);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = app.theme();
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
    let dialog_area =
        animated_dialog_rect(62, 18, area, app, crate::animation::AnimationTag::Dialog);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = app.theme();
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
    let dialog_area =
        animated_dialog_rect(66, 16, area, app, crate::animation::AnimationTag::Dialog);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = app.theme();
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

/// Calculates the clamped Rect for the context menu.
pub fn calculate_context_menu_rect(
    position: (u16, u16),
    item_count: usize,
    viewport: Rect,
) -> Rect {
    let width = 30.min(viewport.width);
    let height = (item_count as u16 + 2).min(viewport.height);

    let mut x = position.0;
    let mut y = position.1;

    if x + width > viewport.x + viewport.width {
        x = (viewport.x + viewport.width).saturating_sub(width);
    }
    if y + height > viewport.y + viewport.height {
        y = (viewport.y + viewport.height).saturating_sub(height);
    }

    x = x.max(viewport.x);
    y = y.max(viewport.y);

    Rect::new(x, y, width, height)
}

/// Calculates the clamped Rect for the context submenu.
pub fn calculate_context_submenu_rect(
    menu_rect: Rect,
    selected: usize,
    item_count: usize,
    viewport: Rect,
) -> Rect {
    let sub_width = 28.min(viewport.width);
    let sub_height = (item_count as u16 + 2).min(viewport.height);

    let mut sub_x = menu_rect.x + menu_rect.width;
    if sub_x + sub_width > viewport.x + viewport.width {
        sub_x = menu_rect.x.saturating_sub(sub_width);
    }
    let mut sub_y = menu_rect.y + selected as u16;
    if sub_y + sub_height > viewport.y + viewport.height {
        sub_y = (viewport.y + viewport.height).saturating_sub(sub_height);
    }

    sub_x = sub_x.max(viewport.x);
    sub_y = sub_y.max(viewport.y);

    Rect::new(sub_x, sub_y, sub_width, sub_height)
}

/// Renders the contextual action popup menu and optional submenu.
pub fn render_context_menu_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let menu = app.context_menu();
    if menu.items.is_empty() || area.width < 10 || area.height < 5 {
        return;
    }

    let theme = app.theme();
    let menu_rect = calculate_context_menu_rect(menu.position, menu.items.len(), area);

    frame.render_widget(Clear, menu_rect);

    let block = Block::default()
        .title(" Actions ")
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.dialog_border);

    let inner = block.inner(menu_rect);
    frame.render_widget(block, menu_rect);

    let mut lines = Vec::new();
    let inner_width = inner.width as usize;
    let inner_height = inner.height as usize;

    let visible_count = inner_height.max(1);
    let start_idx = if menu.selected >= visible_count {
        menu.selected.saturating_sub(visible_count - 1)
    } else {
        0
    };
    let end_idx = (start_idx + visible_count).min(menu.items.len());

    for (index, item) in menu.items[start_idx..end_idx].iter().enumerate() {
        let actual_idx = start_idx + index;
        let is_selected = actual_idx == menu.selected;
        match item {
            ContextMenuItem::Action {
                label, shortcut, ..
            } => {
                let shortcut_str = shortcut.as_deref().unwrap_or("");
                let shortcut_w = display_width(shortcut_str);
                let label_max = inner_width.saturating_sub(shortcut_w + 3);
                let label_disp = truncate_to_width(label, label_max);
                let label_w = display_width(&label_disp);
                let padding = inner_width.saturating_sub(label_w + shortcut_w + 2);

                if is_selected {
                    lines.push(Line::from(vec![
                        Span::styled("▶ ", theme.palette_selected),
                        Span::styled(label_disp, theme.palette_selected),
                        Span::raw(" ".repeat(padding)),
                        Span::styled(shortcut_str.to_string(), theme.palette_shortcut),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(label_disp, theme.palette_unselected),
                        Span::raw(" ".repeat(padding)),
                        Span::styled(shortcut_str.to_string(), theme.dialog_hint),
                    ]));
                }
            }
            ContextMenuItem::More { label, .. } => {
                let arrow = "›";
                let arrow_w = 1;
                let label_max = inner_width.saturating_sub(arrow_w + 3);
                let label_disp = truncate_to_width(label, label_max);
                let label_w = display_width(&label_disp);
                let padding = inner_width.saturating_sub(label_w + arrow_w + 2);

                if is_selected {
                    lines.push(Line::from(vec![
                        Span::styled("▶ ", theme.palette_selected),
                        Span::styled(label_disp, theme.palette_selected),
                        Span::raw(" ".repeat(padding)),
                        Span::styled(arrow.to_string(), theme.palette_shortcut),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(label_disp, theme.palette_unselected),
                        Span::raw(" ".repeat(padding)),
                        Span::styled(arrow.to_string(), theme.dialog_hint),
                    ]));
                }
            }
            ContextMenuItem::Disabled { label, reason } => {
                let label_disp = truncate_to_width(label, inner_width.saturating_sub(4));
                let mut spans = vec![Span::raw("  "), Span::styled(label_disp, theme.dialog_hint)];
                if let Some(r) = reason {
                    let label_w = display_width(label);
                    let avail = inner_width.saturating_sub(label_w + 4);
                    if avail > 6 {
                        spans.push(Span::raw(" "));
                        spans.push(Span::styled(format!("({r})"), theme.dialog_hint));
                    }
                }
                lines.push(Line::from(spans));
            }
            ContextMenuItem::Separator => {
                lines.push(Line::from(vec![Span::styled(
                    "─".repeat(inner_width),
                    theme.footer_separator,
                )]));
            }
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);

    // Render Submenu if `is_more_open`
    if menu.is_more_open
        && let Some(ContextMenuItem::More { items, .. }) = menu.items.get(menu.selected)
        && !items.is_empty()
    {
        let sub_rect = calculate_context_submenu_rect(
            menu_rect,
            menu.selected.saturating_sub(start_idx),
            items.len(),
            area,
        );
        frame.render_widget(Clear, sub_rect);

        let sub_block = Block::default()
            .title(" More ")
            .title_alignment(Alignment::Left)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.dialog_border);

        let sub_inner = sub_block.inner(sub_rect);
        frame.render_widget(sub_block, sub_rect);

        let mut sub_lines = Vec::new();
        let sub_inner_w = sub_inner.width as usize;
        let sub_inner_h = sub_inner.height as usize;
        let sub_visible = sub_inner_h.max(1);
        let sub_start = if menu.more_selected >= sub_visible {
            menu.more_selected.saturating_sub(sub_visible - 1)
        } else {
            0
        };
        let sub_end = (sub_start + sub_visible).min(items.len());

        for (idx, sub_item) in items[sub_start..sub_end].iter().enumerate() {
            let actual_sub_idx = sub_start + idx;
            let is_sub_selected = actual_sub_idx == menu.more_selected;
            match sub_item {
                ContextMenuItem::Action {
                    label, shortcut, ..
                } => {
                    let shortcut_str = shortcut.as_deref().unwrap_or("");
                    let shortcut_w = display_width(shortcut_str);
                    let label_max = sub_inner_w.saturating_sub(shortcut_w + 3);
                    let label_disp = truncate_to_width(label, label_max);
                    let label_w = display_width(&label_disp);
                    let padding = sub_inner_w.saturating_sub(label_w + shortcut_w + 2);

                    if is_sub_selected {
                        sub_lines.push(Line::from(vec![
                            Span::styled("▶ ", theme.palette_selected),
                            Span::styled(label_disp, theme.palette_selected),
                            Span::raw(" ".repeat(padding)),
                            Span::styled(shortcut_str.to_string(), theme.palette_shortcut),
                        ]));
                    } else {
                        sub_lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(label_disp, theme.palette_unselected),
                            Span::raw(" ".repeat(padding)),
                            Span::styled(shortcut_str.to_string(), theme.dialog_hint),
                        ]));
                    }
                }
                ContextMenuItem::Disabled { label, reason } => {
                    let mut spans = vec![
                        Span::raw("  "),
                        Span::styled(
                            truncate_to_width(label, sub_inner_w.saturating_sub(4)),
                            theme.dialog_hint,
                        ),
                    ];
                    if let Some(r) = reason {
                        let label_w = display_width(label);
                        let avail = sub_inner_w.saturating_sub(label_w + 4);
                        if avail > 6 {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled(format!("({r})"), theme.dialog_hint));
                        }
                    }
                    sub_lines.push(Line::from(spans));
                }
                ContextMenuItem::Separator => {
                    sub_lines.push(Line::from(vec![Span::styled(
                        "─".repeat(sub_inner_w),
                        theme.footer_separator,
                    )]));
                }
                ContextMenuItem::More { label, .. } => {
                    let arrow = "›";
                    let arrow_w = 1;
                    let label_max = sub_inner_w.saturating_sub(arrow_w + 3);
                    let label_disp = truncate_to_width(label, label_max);
                    let label_w = display_width(&label_disp);
                    let padding = sub_inner_w.saturating_sub(label_w + arrow_w + 2);

                    if is_sub_selected {
                        sub_lines.push(Line::from(vec![
                            Span::styled("▶ ", theme.palette_selected),
                            Span::styled(label_disp, theme.palette_selected),
                            Span::raw(" ".repeat(padding)),
                            Span::styled(arrow.to_string(), theme.palette_shortcut),
                        ]));
                    } else {
                        sub_lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(label_disp, theme.palette_unselected),
                            Span::raw(" ".repeat(padding)),
                            Span::styled(arrow.to_string(), theme.dialog_hint),
                        ]));
                    }
                }
            }
        }

        frame.render_widget(Paragraph::new(sub_lines), sub_inner);
    }
}

/// Renders the centered Quick Preview modal overlay.
pub fn render_quick_preview_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let preview_area = centered_rect(
        (area.width.saturating_sub(4))
            .min((area.width * 90) / 100)
            .max(40),
        (area.height.saturating_sub(2))
            .min((area.height * 90) / 100)
            .max(14),
        area,
    );
    if preview_area.width < 4 || preview_area.height < 4 {
        return;
    }

    let theme = app.theme();
    frame.render_widget(Clear, preview_area);

    let title = match (
        app.preview()
            .path()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str()),
        app.preview().content(),
    ) {
        (Some(name), _) => format!(" [ Quick Preview: {name} • Esc/Space: Close ] "),
        (None, _) => " [ Quick Preview • Esc/Space: Close ] ".to_string(),
    };

    let block = Block::default()
        .title(title)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.preview_border);

    let inner = block.inner(preview_area);
    frame.render_widget(block, preview_area);

    if inner.height > 0 && inner.width > 0 {
        crate::ui::preview::render_preview_inner_with_theme(
            frame,
            inner,
            app.preview().content(),
            &theme,
        );
    }
}

/// Renders the compact Operation Center progress dialog for active/paused file operations.
pub fn render_operation_progress_dialog(
    frame: &mut Frame,
    area: Rect,
    metrics: &crate::operations::OperationMetrics,
    theme: &Theme,
) {
    let dialog_area = centered_rect(60, 10, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    frame.render_widget(Clear, dialog_area);

    let title_status = if metrics.is_paused {
        " OPERATION PAUSED "
    } else if metrics.is_cancelled {
        " CANCELLING OPERATION... "
    } else {
        " OPERATION IN PROGRESS "
    };

    let block = Block::default()
        .title(title_status)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if metrics.is_paused {
            theme.notify_warning
        } else {
            theme.tab_active_focused
        });

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let mut lines = Vec::new();

    // Source -> Destination
    let src_str = metrics.current_source.as_deref().unwrap_or("...");
    let dst_str = metrics.current_destination.as_deref().unwrap_or("...");
    let transfer_info = format!("{src_str} → {dst_str}");
    let transfer_trunc = truncate_to_width(&transfer_info, inner.width as usize);
    lines.push(Line::from(Span::styled(
        transfer_trunc,
        theme
            .preview_metadata_value
            .add_modifier(ratatui::style::Modifier::BOLD),
    )));
    lines.push(Line::default());

    // Progress bar
    let pct = metrics.percentage();
    let bar_width = (inner.width as usize).saturating_sub(10).max(10);
    let bar = metrics.render_progress_bar(bar_width);
    lines.push(Line::from(vec![
        Span::styled(bar, theme.tab_active_focused),
        Span::styled(format!("  {:>3.0}%", pct), theme.header_path),
    ]));

    // Throughput & ETA
    let progress_stats = format!(
        "{} • {} • {}",
        metrics.format_bytes_progress(),
        metrics.format_speed(),
        metrics.format_eta()
    );
    let stats_trunc = truncate_to_width(&progress_stats, inner.width as usize);
    lines.push(Line::from(Span::styled(
        stats_trunc,
        theme.preview_line_number,
    )));
    lines.push(Line::default());

    // Controls: [Pause / Resume] [Cancel]
    let pause_label = if metrics.is_paused {
        "[Resume]"
    } else {
        "[Pause]"
    };
    lines.push(Line::from(vec![
        Span::styled(format!(" {pause_label} "), theme.dialog_button_inactive),
        Span::raw("   "),
        Span::styled(" [Cancel] ", theme.dialog_button_active),
    ]));

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, inner);
}

/// Renders the interactive conflict resolution dialog.
pub fn render_conflict_dialog(
    frame: &mut Frame,
    area: Rect,
    conflict: &crate::operations::ConflictPrompt,
    theme: &Theme,
) {
    let dialog_area = centered_rect(66, 14, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Destination File Collision ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.notify_warning);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let mut lines = Vec::new();
    let src_name = conflict
        .source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("source");
    let src_size_str = crate::preview::format_size(conflict.source_size);
    let dst_size_str = crate::preview::format_size(conflict.destination_size);

    lines.push(Line::from(Span::styled(
        format!("File '{src_name}' already exists at destination."),
        theme
            .preview_metadata_value
            .add_modifier(ratatui::style::Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        format!("  Source:      {src_size_str}"),
        theme.preview_line_number,
    )));
    lines.push(Line::from(Span::styled(
        format!("  Destination: {dst_size_str}"),
        theme.preview_line_number,
    )));
    lines.push(Line::default());

    // Option buttons row 1
    let mut row1_spans = Vec::new();
    let options = crate::operations::ConflictResolution::ALL_OPTIONS;
    for (idx, opt) in options[0..3].iter().enumerate() {
        let is_selected = idx == conflict.selected_option_index;
        let btn_style = if is_selected {
            theme.dialog_button_active
        } else {
            theme.dialog_button_inactive
        };
        row1_spans.push(Span::styled(format!(" [{}] ", opt.label()), btn_style));
        row1_spans.push(Span::raw("  "));
    }
    lines.push(Line::from(row1_spans));

    lines.push(Line::default());

    // Option buttons row 2
    let mut row2_spans = Vec::new();
    for (idx, opt) in options[3..6].iter().enumerate() {
        let actual_idx = idx + 3;
        let is_selected = actual_idx == conflict.selected_option_index;
        let btn_style = if is_selected {
            theme.dialog_button_active
        } else {
            theme.dialog_button_inactive
        };
        row2_spans.push(Span::styled(format!(" [{}] ", opt.label()), btn_style));
        row2_spans.push(Span::raw("  "));
    }
    lines.push(Line::from(row2_spans));

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, inner);
}

/// Renders the error recovery dialog when a file operation encounters an I/O error.
pub fn render_error_recovery_dialog(
    frame: &mut Frame,
    area: Rect,
    err_prompt: &crate::operations::OperationErrorPrompt,
    theme: &Theme,
) {
    let dialog_area = centered_rect(60, 11, area);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(format!(" {} Operation Failed ", theme.symbols.error_prefix))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.notify_error_text);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let mut lines = Vec::new();
    let path_str = err_prompt.path.display().to_string();
    let path_trunc = truncate_to_width(&path_str, inner.width as usize);

    lines.push(Line::from(Span::styled(
        &err_prompt.error_message,
        theme
            .notify_error_text
            .add_modifier(ratatui::style::Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        path_trunc,
        theme.preview_line_number,
    )));
    lines.push(Line::default());

    let recovery_options = ["Retry", "Skip", "Cancel"];
    let mut btn_spans = Vec::new();
    for (idx, label) in recovery_options.iter().enumerate() {
        let is_selected = idx == err_prompt.selected_option;
        let (btn_text, btn_style) = if is_selected {
            (format!(" ▶ [{label}] ◀ "), theme.dialog_button_active)
        } else {
            (format!("   [{label}]   "), theme.dialog_button_inactive)
        };
        btn_spans.push(Span::styled(btn_text, btn_style));
        btn_spans.push(Span::raw(" "));
    }
    lines.push(Line::from(btn_spans));

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, inner);
}

/// Renders the Theme Selector dialog with live preview support.
pub fn render_theme_selector_dialog(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area =
        animated_dialog_rect(70, 16, area, app, crate::animation::AnimationTag::Dialog);
    if dialog_area.width < 4 || dialog_area.height < 4 {
        return;
    }

    let theme = app.theme();
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" 🎨 Theme Selector ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.palette_shortcut);

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let selector_state = app.theme_selector();
    let selected_idx = selector_state.selected_index();
    let themes = ThemeId::all();
    let active_id = app.active_theme_id();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Top info/instruction
            Constraint::Min(1),    // Theme list
            Constraint::Length(1), // Apply / Cancel buttons
            Constraint::Length(1), // Footer hint
        ])
        .split(inner);

    // Top instruction
    let header_text = if inner.width >= 40 {
        "Select a theme to preview live:"
    } else {
        "Choose theme:"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(header_text, theme.dialog_hint)),
        chunks[0],
    );

    // Theme list
    let list_area = chunks[1];
    let list_height = list_area.height as usize;
    let list_width = list_area.width as usize;

    let scroll_offset = if selected_idx >= list_height {
        selected_idx.saturating_sub(list_height - 1)
    } else {
        0
    };

    let mut lines = Vec::new();
    for (idx, &theme_id) in themes
        .iter()
        .skip(scroll_offset)
        .take(list_height)
        .enumerate()
    {
        let actual_idx = scroll_offset + idx;
        let is_selected = actual_idx == selected_idx;
        let is_active = theme_id == active_id;

        // Active indicator: ● if active/saved theme, otherwise space
        let active_dot = if is_active {
            Span::styled("● ", theme.palette_shortcut)
        } else {
            Span::styled("  ", theme.dialog_hint)
        };

        let pointer = if is_selected {
            Span::styled("▶ ", theme.palette_shortcut)
        } else {
            Span::raw("  ")
        };

        let name_style = if is_selected {
            theme.palette_selected
        } else {
            theme.palette_unselected
        };

        let name_str = format!("{:<16}", theme_id.name());
        let mut spans = vec![pointer, active_dot, Span::styled(name_str, name_style)];

        // Color swatches if width allows
        if list_width >= 45 {
            let item_theme = Theme::for_id(theme_id);
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                "■",
                ratatui::style::Style::default().fg(item_theme.palette.primary),
            ));
            spans.push(Span::styled(
                "■",
                ratatui::style::Style::default().fg(item_theme.palette.accent),
            ));
            spans.push(Span::styled(
                "■",
                ratatui::style::Style::default().fg(item_theme.palette.secondary),
            ));
            spans.push(Span::styled(
                "■",
                ratatui::style::Style::default().fg(item_theme.palette.surface_alt),
            ));
            spans.push(Span::styled(
                "■",
                ratatui::style::Style::default().fg(item_theme.palette.success),
            ));
            spans.push(Span::styled(
                "■",
                ratatui::style::Style::default().fg(item_theme.palette.warning),
            ));
            spans.push(Span::styled(
                "■",
                ratatui::style::Style::default().fg(item_theme.palette.error),
            ));
            spans.push(Span::raw(" "));
        }

        // Short description if wide enough
        if list_width >= 62 {
            let desc = theme_id.description();
            let avail = list_width.saturating_sub(34);
            let trunc_desc = truncate_to_width(desc, avail);
            spans.push(Span::styled(
                format!(" {trunc_desc}"),
                theme.palette_description,
            ));
        }

        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), list_area);

    // Apply / Cancel buttons
    let btn_spans = vec![
        Span::styled(" [ Enter: Apply ] ", theme.dialog_button_active),
        Span::raw("   "),
        Span::styled(" [ Esc: Cancel ] ", theme.dialog_button_inactive),
    ];
    frame.render_widget(
        Paragraph::new(Line::from(btn_spans)).alignment(Alignment::Center),
        chunks[2],
    );

    // Footer navigation hint
    let footer_hint = "↑/↓: Navigate • Enter: Apply • Esc: Cancel";
    frame.render_widget(
        Paragraph::new(Span::styled(footer_hint, theme.dialog_hint)).alignment(Alignment::Center),
        chunks[3],
    );
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
        assert!(text.contains("Command Center"));
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
        assert!(text.contains("Quick Switcher"));
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
        assert!(text.contains("Project Overview"));
    }

    #[test]
    fn render_storage_vision_dialog_renders_without_panic() {
        let backend = TestBackend::new(84, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        app.handle_action(crate::app::actions::Action::StorageVision);

        terminal
            .draw(|f| {
                render_storage_vision_dialog(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..24)
            .flat_map(|y| (0..84).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("Storage Vision"));
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
