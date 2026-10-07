//! Mouse input handling and hit testing.
//!
//! Translates terminal mouse events into semantic actions and application state updates
//! based on geometry derived from the responsive layout engine.
//!
//! This module never performs filesystem operations directly.

use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::app::actions::Action;
use crate::app::modes::Mode;
use crate::app::state::{ActivePane, App, ContextMenuItem};
use crate::layout::geometry::{MainLayout, ScreenLayout};
use crate::ui::dialogs::{
    calculate_command_palette_rect, calculate_context_menu_rect, calculate_context_submenu_rect,
    calculate_smart_jump_rect, calculate_theme_selector_rect,
};
use crate::ui::footer::action_hit_ranges;
use crate::ui::theme::ThemeId;

/// Maximum duration between two clicks on the same row to qualify as a double-click.
pub const DOUBLE_CLICK_THRESHOLD: Duration = Duration::from_millis(400);

/// Tracks the previous click for double-click detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LastClick {
    pane: ActivePane,
    index: usize,
    time: Instant,
}

/// Tracks mouse state across events, such as double-click timing.
#[derive(Debug, Default, Clone)]
pub struct MouseTracker {
    last_click: Option<LastClick>,
}

impl MouseTracker {
    /// Creates a new, empty mouse tracker.
    pub fn new() -> Self {
        Self { last_click: None }
    }

    /// Records a left click on `(pane, index)` at time `now`.
    ///
    /// Returns `true` if this click forms a valid double-click with the immediately
    /// preceding click, resetting the tracker so subsequent clicks start a new sequence.
    pub fn record_left_click(&mut self, pane: ActivePane, index: usize, now: Instant) -> bool {
        if let Some(last) = self.last_click.take()
            && last.pane == pane
            && last.index == index
            && now.saturating_duration_since(last.time) <= DOUBLE_CLICK_THRESHOLD
        {
            return true;
        }

        self.last_click = Some(LastClick {
            pane,
            index,
            time: now,
        });
        false
    }

    /// Clears any recorded click state.
    pub fn clear(&mut self) {
        self.last_click = None;
    }
}

/// Identifies which file pane (if any) contains the coordinate `(column, row)`.
pub fn hit_test_pane(
    column: u16,
    row: u16,
    terminal_area: Rect,
    active_pane: ActivePane,
) -> Option<(ActivePane, Rect)> {
    let layout = ScreenLayout::calculate(terminal_area);

    match layout.main() {
        MainLayout::Single { pane } => {
            if contains(pane, column, row) {
                Some((active_pane, pane))
            } else {
                None
            }
        }
        MainLayout::Two { left, right } => {
            if contains(left, column, row) {
                Some((ActivePane::Left, left))
            } else if contains(right, column, row) {
                Some((ActivePane::Right, right))
            } else {
                None
            }
        }
        MainLayout::Three { left, right, .. } => {
            if contains(left, column, row) {
                Some((ActivePane::Left, left))
            } else if contains(right, column, row) {
                Some((ActivePane::Right, right))
            } else {
                None
            }
        }
    }
}

/// Calculates the clicked tab index if the coordinate falls on a tab in the pane header.
pub fn hit_test_tab(
    column: u16,
    row: u16,
    pane_rect: Rect,
    pane: &crate::app::state::Pane,
    is_active: bool,
) -> Option<usize> {
    if row != pane_rect.y {
        return None;
    }
    for (index, start_x, end_x) in crate::ui::panes::tab_hit_ranges(pane_rect, pane, is_active) {
        if column >= start_x && column < end_x {
            return Some(index);
        }
    }
    None
}

/// Calculates the clicked item index inside a pane's file-list content area.
///
/// Returns `Some(index)` if the coordinate falls directly on a displayed entry row,
/// or `None` if the coordinate is on the border, outside the pane, or on empty space
/// below the last entry.
pub fn hit_test_row(
    column: u16,
    row: u16,
    pane_rect: Rect,
    scroll_offset: usize,
    total_count: usize,
) -> Option<usize> {
    // Inner area accounts for borders (1 cell padding on each side).
    let inner_x = pane_rect.x.saturating_add(1);
    let inner_y = pane_rect.y.saturating_add(1);
    let inner_width = pane_rect.width.saturating_sub(2);
    let inner_height = pane_rect.height.saturating_sub(2);

    if inner_width == 0 || inner_height == 0 {
        return None;
    }

    let inner = Rect::new(inner_x, inner_y, inner_width, inner_height);
    if !contains(inner, column, row) {
        return None;
    }

    let relative_row = row.saturating_sub(inner.y) as usize;
    let clicked_index = scroll_offset.saturating_add(relative_row);

    if clicked_index < total_count {
        Some(clicked_index)
    } else {
        None
    }
}

/// Whether `rect` contains `(x, y)`.
fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

/// Applies a terminal mouse event to the application state with a specified timestamp.
pub fn handle_mouse_event_at(
    event: MouseEvent,
    app: &mut App,
    terminal_area: Rect,
    tracker: &mut MouseTracker,
    now: Instant,
) {
    if app.mode() == Mode::Boot {
        if matches!(event.kind, MouseEventKind::Down(_)) {
            app.skip_boot();
        }
        return;
    }

    if app.mode() == Mode::ContextMenu {
        handle_context_menu_mouse(event, app, terminal_area);
        return;
    }

    if app.mode() == Mode::CommandPalette {
        handle_command_palette_mouse(event, app, terminal_area);
        return;
    }

    if app.mode() == Mode::SmartJump {
        handle_smart_jump_mouse(event, app, terminal_area);
        return;
    }

    if app.mode() == Mode::ThemeSelector {
        handle_theme_selector_mouse(event, app, terminal_area);
        return;
    }

    if app.mode() == Mode::ProjectCockpit {
        handle_project_cockpit_mouse(event, app, terminal_area);
        return;
    }

    if app.mode() == Mode::Preview {
        if matches!(event.kind, MouseEventKind::Down(_)) {
            app.leave_temporary_mode();
        }
        return;
    }

    if app.mode() == Mode::StorageVision {
        handle_storage_vision_mouse(event, app, terminal_area);
        return;
    }

    if app.operation_manager().active_conflict.is_some() {
        if let MouseEventKind::Down(MouseButton::Left) = event.kind {
            let mgr = app.operation_manager_mut();
            if let Some(conflict) = mgr.active_conflict.as_mut() {
                conflict.next_option();
            }
        }
        return;
    }

    if app.operation_manager().active_metrics.is_some() {
        if let MouseEventKind::Down(MouseButton::Left) = event.kind {
            let dialog_rect = crate::ui::dialogs::centered_rect(60, 10, terminal_area);
            if contains(dialog_rect, event.column, event.row) {
                app.operation_manager_mut().toggle_pause();
            }
        }
        return;
    }

    if app.mode() != Mode::Normal && app.mode() != Mode::Terminal {
        return;
    }

    let layout = ScreenLayout::calculate(terminal_area);
    let term_rect = layout.terminal();

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // First check if the header Command Center button was clicked
            let header_rect = layout.header();
            let platform = crate::input::platform::Platform::current();
            if let Some((start_x, end_x, y)) =
                crate::ui::header::command_center_hit_range(header_rect, platform)
                && event.row == y
                && event.column >= start_x
                && event.column < end_x
            {
                tracker.clear();
                app.handle_action(Action::CommandPalette);
                return;
            }

            // Check if a breadcrumb segment was clicked in the header
            if let Some(target_path) =
                crate::ui::header::breadcrumb_hit_segment(header_rect, app, event.column, event.row)
            {
                tracker.clear();
                let active = app.active_pane();
                let outcome = app.open_in(active, target_path);
                app.report_navigation(outcome);
                app.refresh_preview();
                return;
            }

            // Next check if an adaptive action button in the footer was clicked
            if app.mode() == Mode::Normal {
                for (action, start_x, end_x, y) in action_hit_ranges(layout.footer(), app) {
                    if event.row == y && event.column >= start_x && event.column < end_x {
                        tracker.clear();
                        app.handle_action(action);
                        return;
                    }
                }
            }

            let active = app.active_pane();
            if let Some((pane, pane_rect)) =
                hit_test_pane(event.column, event.row, terminal_area, active)
            {
                if app.mode() == Mode::Terminal {
                    app.handle_action(Action::FocusFileManager);
                }

                let scroll_offset = app.pane(pane).scroll_offset();
                let total_count = app.pane(pane).visible_count();

                if let Some(tab_index) = hit_test_tab(
                    event.column,
                    event.row,
                    pane_rect,
                    app.pane(pane),
                    active == pane,
                ) {
                    tracker.clear();
                    app.select_tab_in_pane(pane, tab_index);
                } else if let Some(index) = hit_test_row(
                    event.column,
                    event.row,
                    pane_rect,
                    scroll_offset,
                    total_count,
                ) {
                    let has_shift = event
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::SHIFT);
                    let has_toggle = event
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL)
                        || event
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::SUPER);

                    if has_shift {
                        app.set_active_pane(pane);
                        app.select_range_to(pane, index);
                    } else if has_toggle {
                        app.set_active_pane(pane);
                        app.pane_mut(pane).toggle_selection(index);
                    } else {
                        app.select_entry(pane, index);
                    }

                    let is_double = tracker.record_left_click(pane, index, now);
                    if is_double {
                        if app.pane(pane).is_directory_at(index) {
                            app.handle_action(Action::Open);
                        } else {
                            app.handle_action(Action::Preview);
                        }
                    }
                } else {
                    // Clicked empty area inside pane -> deselect all
                    tracker.clear();
                    app.set_active_pane(pane);
                    app.pane_mut(pane).deselect_all();
                }
            } else if contains(term_rect, event.column, event.row) {
                tracker.clear();
                app.handle_action(Action::FocusTerminal);
            } else {
                // Clicked outside pane (header, footer, preview, separator)
                tracker.clear();
            }
        }

        MouseEventKind::Down(MouseButton::Right) => {
            tracker.clear();
            let active = app.active_pane();
            if let Some((pane, pane_rect)) =
                hit_test_pane(event.column, event.row, terminal_area, active)
            {
                if app.mode() == Mode::Terminal {
                    app.handle_action(Action::FocusFileManager);
                }
                app.set_active_pane(pane);

                let scroll_offset = app.pane(pane).scroll_offset();
                let total_count = app.pane(pane).visible_count();

                if let Some(index) = hit_test_row(
                    event.column,
                    event.row,
                    pane_rect,
                    scroll_offset,
                    total_count,
                ) {
                    // If clicked entry is not part of multi-selection, select it singly
                    if let Some(path) = app.pane(pane).path_at(index) {
                        let path_buf = path.to_path_buf();
                        if !app.pane(pane).selected_paths().contains(&path_buf) {
                            app.select_entry(pane, index);
                        }
                    } else {
                        app.select_entry(pane, index);
                    }
                } else {
                    // Clicked on empty space inside pane
                    app.pane_mut(pane).deselect_all();
                }

                // Open context menu at mouse coordinates
                app.open_context_menu((event.column, event.row));
            } else if contains(term_rect, event.column, event.row) {
                // Do not steal right-click from terminal
                app.handle_action(Action::FocusTerminal);
            }
        }

        MouseEventKind::ScrollUp => {
            if contains(term_rect, event.column, event.row) {
                app.handle_action(Action::ScrollTerminalUp);
            } else {
                let active = app.active_pane();
                if let Some((pane, _)) =
                    hit_test_pane(event.column, event.row, terminal_area, active)
                {
                    if app.mode() == Mode::Terminal {
                        app.handle_action(Action::FocusFileManager);
                    }
                    app.set_active_pane(pane);
                }
                app.handle_action(Action::MoveUp);
            }
        }

        MouseEventKind::ScrollDown => {
            if contains(term_rect, event.column, event.row) {
                app.handle_action(Action::ScrollTerminalDown);
            } else {
                let active = app.active_pane();
                if let Some((pane, _)) =
                    hit_test_pane(event.column, event.row, terminal_area, active)
                {
                    if app.mode() == Mode::Terminal {
                        app.handle_action(Action::FocusFileManager);
                    }
                    app.set_active_pane(pane);
                }
                app.handle_action(Action::MoveDown);
            }
        }

        _ => {
            // Ignore mouse up, move, drag, middle button, etc.
        }
    }
}

/// Handles mouse interactions when the context menu is open.
fn handle_context_menu_mouse(event: MouseEvent, app: &mut App, terminal_area: Rect) {
    let menu = app.context_menu();
    if menu.items.is_empty() {
        app.close_context_menu();
        return;
    }

    let menu_rect = calculate_context_menu_rect(menu.position, menu.items.len(), terminal_area);
    let is_more_open = menu.is_more_open;
    let selected_idx = menu.selected;

    // Check submenu rect if open
    let sub_rect = if is_more_open {
        if let Some(ContextMenuItem::More { items, .. }) = menu.items.get(selected_idx) {
            if !items.is_empty() {
                Some(calculate_context_submenu_rect(
                    menu_rect,
                    selected_idx,
                    items.len(),
                    terminal_area,
                ))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // Check submenu first if open
            if let Some(srect) = sub_rect {
                let inner_x = srect.x.saturating_add(1);
                let inner_y = srect.y.saturating_add(1);
                let inner_w = srect.width.saturating_sub(2);
                let inner_h = srect.height.saturating_sub(2);
                let s_inner = Rect::new(inner_x, inner_y, inner_w, inner_h);

                if contains(s_inner, event.column, event.row) {
                    let rel_row = event.row.saturating_sub(inner_y) as usize;
                    if let Some(ContextMenuItem::More { items, .. }) =
                        app.context_menu().items.get(selected_idx)
                        && rel_row < items.len()
                    {
                        app.context_menu_mut().more_selected = rel_row;
                        if let Some(action) = app.context_menu().selected_action() {
                            app.close_context_menu();
                            app.handle_action(action);
                            return;
                        }
                    }
                    return;
                }
            }

            // Check main menu
            let inner_x = menu_rect.x.saturating_add(1);
            let inner_y = menu_rect.y.saturating_add(1);
            let inner_w = menu_rect.width.saturating_sub(2);
            let inner_h = menu_rect.height.saturating_sub(2);
            let m_inner = Rect::new(inner_x, inner_y, inner_w, inner_h);

            if contains(m_inner, event.column, event.row) {
                let rel_row = event.row.saturating_sub(inner_y) as usize;
                if rel_row < app.context_menu().items.len() {
                    match &app.context_menu().items[rel_row] {
                        ContextMenuItem::More { .. } => {
                            app.context_menu_mut().selected = rel_row;
                            app.context_menu_mut().open_more();
                        }
                        ContextMenuItem::Action { .. } => {
                            app.context_menu_mut().selected = rel_row;
                            if let Some(action) = app.context_menu().selected_action() {
                                app.close_context_menu();
                                app.handle_action(action);
                            }
                        }
                        _ => {}
                    }
                    return;
                }
            }

            // Clicked outside context menu -> close it
            app.close_context_menu();
        }

        MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
            // Update highlight on hover
            if let Some(srect) = sub_rect {
                let inner_x = srect.x.saturating_add(1);
                let inner_y = srect.y.saturating_add(1);
                let inner_w = srect.width.saturating_sub(2);
                let inner_h = srect.height.saturating_sub(2);
                let s_inner = Rect::new(inner_x, inner_y, inner_w, inner_h);

                if contains(s_inner, event.column, event.row) {
                    let rel_row = event.row.saturating_sub(inner_y) as usize;
                    if let Some(ContextMenuItem::More { items, .. }) =
                        app.context_menu().items.get(selected_idx)
                        && rel_row < items.len()
                    {
                        app.context_menu_mut().more_selected = rel_row;
                    }
                    return;
                }
            }

            let inner_x = menu_rect.x.saturating_add(1);
            let inner_y = menu_rect.y.saturating_add(1);
            let inner_w = menu_rect.width.saturating_sub(2);
            let inner_h = menu_rect.height.saturating_sub(2);
            let m_inner = Rect::new(inner_x, inner_y, inner_w, inner_h);

            if contains(m_inner, event.column, event.row) {
                let rel_row = event.row.saturating_sub(inner_y) as usize;
                if rel_row < app.context_menu().items.len() {
                    app.context_menu_mut().selected = rel_row;
                }
            }
        }

        MouseEventKind::ScrollUp => {
            app.context_menu_mut().move_up();
        }

        MouseEventKind::ScrollDown => {
            app.context_menu_mut().move_down();
        }

        MouseEventKind::Down(MouseButton::Right) => {
            let layout = ScreenLayout::calculate(terminal_area);
            let active = app.active_pane();
            let pane_hit = hit_test_pane(event.column, event.row, terminal_area, active);
            let term_rect = layout.terminal();

            if let Some((pane, pane_rect)) = pane_hit {
                if app.mode() == Mode::Terminal {
                    app.handle_action(Action::FocusFileManager);
                }
                app.set_active_pane(pane);

                let scroll_offset = app.pane(pane).scroll_offset();
                let total_count = app.pane(pane).visible_count();

                if let Some(index) = hit_test_row(
                    event.column,
                    event.row,
                    pane_rect,
                    scroll_offset,
                    total_count,
                ) {
                    if let Some(path) = app.pane(pane).path_at(index) {
                        let path_buf = path.to_path_buf();
                        if !app.pane(pane).selected_paths().contains(&path_buf) {
                            app.select_entry(pane, index);
                        }
                    } else {
                        app.select_entry(pane, index);
                    }
                } else {
                    app.pane_mut(pane).deselect_all();
                }

                app.open_context_menu((event.column, event.row));
            } else if contains(term_rect, event.column, event.row) {
                app.close_context_menu();
                app.handle_action(Action::FocusTerminal);
            } else {
                app.close_context_menu();
            }
        }

        _ => {}
    }
}

/// Handles mouse interactions when the Command Center is open.
fn handle_command_palette_mouse(event: MouseEvent, app: &mut App, terminal_area: Rect) {
    let dialog_rect = calculate_command_palette_rect(terminal_area);
    if dialog_rect.width < 4 || dialog_rect.height < 4 {
        return;
    }

    let inner = dialog_rect.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.height < 4 || inner.width == 0 {
        return;
    }

    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(1), // Query input box
            ratatui::layout::Constraint::Length(1), // Separator
            ratatui::layout::Constraint::Min(1),    // Results list
            ratatui::layout::Constraint::Length(1), // Footer hint
        ])
        .split(inner);

    let list_rect = chunks[2];

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if contains(list_rect, event.column, event.row) {
                let rel_row = event.row.saturating_sub(list_rect.y) as usize;
                let selected = app.command_palette().selected_index();
                let list_height = list_rect.height as usize;
                let scroll_offset = if selected >= list_height {
                    selected.saturating_sub(list_height - 1)
                } else {
                    0
                };
                let clicked_idx = scroll_offset + rel_row;
                if clicked_idx < app.command_palette().entries().len() {
                    app.command_palette_mut().select(clicked_idx);
                    if let Some(action) = app.confirm_modal() {
                        app.handle_action(action);
                    }
                }
            } else if !contains(dialog_rect, event.column, event.row) {
                // Clicked outside Command Center dialog -> close it
                app.close_modal();
            }
        }
        MouseEventKind::ScrollUp => {
            app.command_palette_mut().select_previous();
        }
        MouseEventKind::ScrollDown => {
            app.command_palette_mut().select_next();
        }
        MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left)
            if contains(list_rect, event.column, event.row) =>
        {
            let rel_row = event.row.saturating_sub(list_rect.y) as usize;
            let selected = app.command_palette().selected_index();
            let list_height = list_rect.height as usize;
            let scroll_offset = if selected >= list_height {
                selected.saturating_sub(list_height - 1)
            } else {
                0
            };
            let hovered_idx = scroll_offset + rel_row;
            if hovered_idx < app.command_palette().entries().len() {
                app.command_palette_mut().select(hovered_idx);
            }
        }
        _ => {}
    }
}

/// Handles mouse interactions when the Quick Switcher is open.
fn handle_smart_jump_mouse(event: MouseEvent, app: &mut App, terminal_area: Rect) {
    let dialog_rect = calculate_smart_jump_rect(terminal_area);
    if dialog_rect.width < 4 || dialog_rect.height < 4 {
        return;
    }

    let inner = dialog_rect.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.height < 4 || inner.width == 0 {
        return;
    }

    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(1), // Query input box
            ratatui::layout::Constraint::Length(1), // Separator
            ratatui::layout::Constraint::Min(1),    // Results list
            ratatui::layout::Constraint::Length(1), // Footer hint
        ])
        .split(inner);

    let list_rect = chunks[2];

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if contains(list_rect, event.column, event.row) {
                let rel_row = event.row.saturating_sub(list_rect.y) as usize;
                let selected = app.smart_jump().selected_index();
                let list_height = list_rect.height as usize;
                let scroll_offset = if selected >= list_height {
                    selected.saturating_sub(list_height - 1)
                } else {
                    0
                };
                let clicked_idx = scroll_offset + rel_row;
                if clicked_idx < app.smart_jump().filtered_items().len() {
                    app.smart_jump_mut().select(clicked_idx);
                    if let Some(action) = app.confirm_modal() {
                        app.handle_action(action);
                    }
                }
            } else if !contains(dialog_rect, event.column, event.row) {
                // Clicked outside Quick Switcher dialog -> close it
                app.close_modal();
            }
        }
        MouseEventKind::ScrollUp => {
            app.smart_jump_mut().select_previous();
        }
        MouseEventKind::ScrollDown => {
            app.smart_jump_mut().select_next();
        }
        MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left)
            if contains(list_rect, event.column, event.row) =>
        {
            let rel_row = event.row.saturating_sub(list_rect.y) as usize;
            let selected = app.smart_jump().selected_index();
            let list_height = list_rect.height as usize;
            let scroll_offset = if selected >= list_height {
                selected.saturating_sub(list_height - 1)
            } else {
                0
            };
            let hovered_idx = scroll_offset + rel_row;
            if hovered_idx < app.smart_jump().filtered_items().len() {
                app.smart_jump_mut().select(hovered_idx);
            }
        }
        _ => {}
    }
}

/// Handles mouse interactions when the Project Overview / Cockpit dialog is open.
fn handle_project_cockpit_mouse(event: MouseEvent, app: &mut App, terminal_area: Rect) {
    let dialog_rect = crate::ui::dialogs::centered_rect(70, 22, terminal_area);
    if dialog_rect.width < 4 || dialog_rect.height < 4 {
        return;
    }

    let inner = dialog_rect.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.height < 7 || inner.width == 0 {
        return;
    }

    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(5), // Project Info Header
            ratatui::layout::Constraint::Length(1), // Separator
            ratatui::layout::Constraint::Min(4),    // Actions List
            ratatui::layout::Constraint::Length(1), // Footer
        ])
        .split(inner);

    let list_rect = chunks[2];

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if contains(list_rect, event.column, event.row) {
                let rel_row = event.row.saturating_sub(list_rect.y) as usize;
                let selected = app.project_cockpit().selected_index();
                let list_height = list_rect.height as usize;
                let scroll_offset = if selected >= list_height {
                    selected.saturating_sub(list_height - 1)
                } else {
                    0
                };
                let clicked_idx = scroll_offset + rel_row;
                if clicked_idx < app.project_cockpit().actions().len() {
                    app.project_cockpit_mut().select(clicked_idx);
                    if let Some(action) = app.confirm_modal() {
                        app.handle_action(action);
                    }
                }
            } else if !contains(dialog_rect, event.column, event.row) {
                app.close_modal();
            }
        }
        MouseEventKind::ScrollUp => {
            app.project_cockpit_mut().select_previous();
        }
        MouseEventKind::ScrollDown => {
            app.project_cockpit_mut().select_next();
        }
        MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left)
            if contains(list_rect, event.column, event.row) =>
        {
            let rel_row = event.row.saturating_sub(list_rect.y) as usize;
            let selected = app.project_cockpit().selected_index();
            let list_height = list_rect.height as usize;
            let scroll_offset = if selected >= list_height {
                selected.saturating_sub(list_height - 1)
            } else {
                0
            };
            let hovered_idx = scroll_offset + rel_row;
            if hovered_idx < app.project_cockpit().actions().len() {
                app.project_cockpit_mut().select(hovered_idx);
            }
        }
        _ => {}
    }
}

/// Handles mouse interactions when the Theme Selector modal is open.
fn handle_theme_selector_mouse(event: MouseEvent, app: &mut App, terminal_area: Rect) {
    let dialog_rect = calculate_theme_selector_rect(terminal_area);
    if dialog_rect.width < 4 || dialog_rect.height < 4 {
        return;
    }

    let inner = dialog_rect.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.height < 4 || inner.width == 0 {
        return;
    }

    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(1), // Top instruction
            ratatui::layout::Constraint::Min(1),    // Theme list
            ratatui::layout::Constraint::Length(1), // Apply / Cancel buttons
            ratatui::layout::Constraint::Length(1), // Footer hint
        ])
        .split(inner);

    let list_rect = chunks[1];
    let buttons_rect = chunks[2];

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if contains(list_rect, event.column, event.row) {
                let rel_row = event.row.saturating_sub(list_rect.y) as usize;
                let selected = app.theme_selector().selected_index();
                let list_height = list_rect.height as usize;
                let scroll_offset = if selected >= list_height {
                    selected.saturating_sub(list_height - 1)
                } else {
                    0
                };
                let clicked_idx = scroll_offset + rel_row;
                if clicked_idx < ThemeId::all().len() {
                    app.theme_selector_select_index(clicked_idx);
                }
            } else if contains(buttons_rect, event.column, event.row) {
                let mid_x = buttons_rect.x + buttons_rect.width / 2;
                if event.column < mid_x {
                    // Left button: Apply
                    app.apply_theme_selector();
                } else {
                    // Right button: Cancel
                    app.cancel_theme_selector();
                }
            } else if !contains(dialog_rect, event.column, event.row) {
                // Clicked outside modal -> cancel and close
                app.cancel_theme_selector();
            }
        }
        MouseEventKind::ScrollUp => {
            app.theme_selector_move_up();
        }
        MouseEventKind::ScrollDown => {
            app.theme_selector_move_down();
        }
        MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left)
            if contains(list_rect, event.column, event.row) =>
        {
            let rel_row = event.row.saturating_sub(list_rect.y) as usize;
            let selected = app.theme_selector().selected_index();
            let list_height = list_rect.height as usize;
            let scroll_offset = if selected >= list_height {
                selected.saturating_sub(list_height - 1)
            } else {
                0
            };
            let hovered_idx = scroll_offset + rel_row;
            if hovered_idx < ThemeId::all().len() {
                app.theme_selector_select_index(hovered_idx);
            }
        }
        _ => {}
    }
}

/// Handles mouse interactions when Storage Vision is open.
fn handle_storage_vision_mouse(event: MouseEvent, app: &mut App, terminal_area: Rect) {
    let dialog_rect = crate::ui::dialogs::centered_rect(84, 24, terminal_area);
    if dialog_rect.width < 4 || dialog_rect.height < 4 {
        return;
    }

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if !contains(dialog_rect, event.column, event.row) {
                app.close_modal();
                return;
            }
            // If inside dialog, trigger drill down on selected item
            app.storage_vision_mut().drill_down();
        }
        MouseEventKind::ScrollUp => {
            app.storage_vision_mut().move_up();
        }
        MouseEventKind::ScrollDown => {
            app.storage_vision_mut().move_down();
        }
        _ => {}
    }
}

/// Applies a terminal mouse event to the application state using the current wall clock time.
pub fn handle_mouse_event(
    event: MouseEvent,
    app: &mut App,
    terminal_area: Rect,
    tracker: &mut MouseTracker,
) {
    handle_mouse_event_at(event, app, terminal_area, tracker, Instant::now());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use std::fs;
    use std::path::PathBuf;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tv_mouse_test_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn mouse_down(column: u16, row: u16, button: MouseButton) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(button),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn mouse_scroll_up(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn mouse_scroll_down(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn test_hit_test_pane_single_layout() {
        // Single pane layout (narrow terminal < 80 cols, e.g. 60x24)
        let area = Rect::new(0, 0, 60, 24);
        // Header is at row 0 (height 1). Main is rows 1..23. Footer is at row 23.
        let hit = hit_test_pane(10, 5, area, ActivePane::Left);
        assert!(hit.is_some());
        let (pane, rect) = hit.unwrap();
        assert_eq!(pane, ActivePane::Left);
        assert_eq!(rect, Rect::new(0, 1, 60, 15));

        // Click on header
        assert!(hit_test_pane(10, 0, area, ActivePane::Left).is_none());
        // Click on footer
        assert!(hit_test_pane(10, 23, area, ActivePane::Left).is_none());
    }

    #[test]
    fn test_hit_test_pane_two_layout() {
        // Two pane layout (e.g. 100x30)
        let area = Rect::new(0, 0, 100, 30);
        // Left pane: cols 0..50, rows 1..29
        // Right pane: cols 50..100, rows 1..29
        let hit_left = hit_test_pane(20, 10, area, ActivePane::Left);
        assert_eq!(hit_left.map(|(p, _)| p), Some(ActivePane::Left));

        let hit_right = hit_test_pane(70, 10, area, ActivePane::Left);
        assert_eq!(hit_right.map(|(p, _)| p), Some(ActivePane::Right));

        // Outside main layout:
        assert!(hit_test_pane(20, 0, area, ActivePane::Left).is_none()); // header
        assert!(hit_test_pane(70, 29, area, ActivePane::Left).is_none()); // footer
    }

    #[test]
    fn test_hit_test_pane_three_layout() {
        // Wide layout (>= 160 cols, e.g. 180x40)
        let area = Rect::new(0, 0, 180, 40);
        // Left pane: 0..60, Right pane: 60..120, Preview: 120..180
        let hit_left = hit_test_pane(10, 10, area, ActivePane::Left);
        assert_eq!(hit_left.map(|(p, _)| p), Some(ActivePane::Left));

        let hit_right = hit_test_pane(70, 10, area, ActivePane::Left);
        assert_eq!(hit_right.map(|(p, _)| p), Some(ActivePane::Right));

        // In preview area (last 1/3 of width = cols 120..180):
        let hit_preview = hit_test_pane(140, 10, area, ActivePane::Left);
        assert!(
            hit_preview.is_none(),
            "clicks in preview area must not hit a file pane"
        );
    }

    #[test]
    fn test_hit_test_row_calculations() {
        let pane_rect = Rect::new(0, 1, 50, 20);
        // Inner rect is x: 1..49, y: 2..20. (height = 18 rows)

        // Top-left row (row 0)
        assert_eq!(hit_test_row(1, 2, pane_rect, 0, 10), Some(0));
        assert_eq!(hit_test_row(25, 2, pane_rect, 0, 10), Some(0));

        // Row 3 with scroll_offset = 0
        assert_eq!(hit_test_row(10, 5, pane_rect, 0, 10), Some(3));

        // Row 3 with scroll_offset = 5
        assert_eq!(hit_test_row(10, 5, pane_rect, 5, 10), Some(8));

        // Last available entry
        assert_eq!(hit_test_row(10, 2 + 9, pane_rect, 0, 10), Some(9));

        // Empty space below entries (e.g. entry index 10 when total is 10)
        assert_eq!(hit_test_row(10, 2 + 10, pane_rect, 0, 10), None);
    }

    #[test]
    fn test_hit_test_row_boundaries_and_borders() {
        let pane_rect = Rect::new(0, 1, 50, 20);

        // On the top border (y = 1)
        assert_eq!(hit_test_row(25, 1, pane_rect, 0, 10), None);
        // On the bottom border (y = 20)
        assert_eq!(hit_test_row(25, 20, pane_rect, 0, 10), None);
        // On the left border (x = 0)
        assert_eq!(hit_test_row(0, 5, pane_rect, 0, 10), None);
        // On the right border (x = 49)
        assert_eq!(hit_test_row(49, 5, pane_rect, 0, 10), None);

        // Outside pane
        assert_eq!(hit_test_row(60, 5, pane_rect, 0, 10), None);
        assert_eq!(hit_test_row(25, 0, pane_rect, 0, 10), None);
    }

    #[test]
    fn test_hit_test_row_tiny_rects() {
        let zero_rect = Rect::new(0, 0, 0, 0);
        assert_eq!(hit_test_row(0, 0, zero_rect, 0, 5), None);

        let tiny_rect_1x1 = Rect::new(0, 0, 1, 1);
        assert_eq!(hit_test_row(0, 0, tiny_rect_1x1, 0, 5), None);

        let tiny_rect_2x2 = Rect::new(0, 0, 2, 2);
        assert_eq!(hit_test_row(1, 1, tiny_rect_2x2, 0, 5), None);
    }

    #[test]
    fn test_empty_pane_hit_testing() {
        let pane_rect = Rect::new(0, 1, 50, 20);
        // Total entries = 0
        assert_eq!(hit_test_row(10, 5, pane_rect, 0, 0), None);
    }

    #[test]
    fn test_left_click_selection_and_pane_activation() {
        let dir = test_dir("click_select");
        fs::write(dir.join("alpha.txt"), "a").unwrap();
        fs::write(dir.join("beta.txt"), "b").unwrap();
        fs::write(dir.join("gamma.txt"), "g").unwrap();

        let mut app = App::at(dir.clone()).unwrap();
        app.open_in(ActivePane::Right, dir).unwrap();
        let mut tracker = MouseTracker::new();
        let terminal_area = Rect::new(0, 0, 100, 30);
        let now = Instant::now();

        // Right pane click selects and activates right pane
        // Right pane inner is at x: 51..99, y: 2..29
        let click_right_row1 = mouse_down(60, 3, MouseButton::Left); // Row 1 (beta.txt)
        handle_mouse_event_at(click_right_row1, &mut app, terminal_area, &mut tracker, now);

        assert_eq!(app.active_pane(), ActivePane::Right);
        assert_eq!(app.pane(ActivePane::Right).selected_index(), Some(1));

        // Left pane click switches back to left pane and selects row 2 (gamma.txt)
        let click_left_row2 = mouse_down(10, 4, MouseButton::Left); // Row 2 (gamma.txt)
        handle_mouse_event_at(click_left_row2, &mut app, terminal_area, &mut tracker, now);

        assert_eq!(app.active_pane(), ActivePane::Left);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(2));
    }

    #[test]
    fn test_single_click_does_not_open_directory() {
        let dir = test_dir("click_dir_no_open");
        let sub = dir.join("subdir");
        fs::create_dir_all(&sub).unwrap();

        let mut app = App::at(dir.clone()).unwrap();
        let mut tracker = MouseTracker::new();
        let terminal_area = Rect::new(0, 0, 100, 30);
        let now = Instant::now();

        let click = mouse_down(10, 2, MouseButton::Left); // Row 0 (subdir)
        handle_mouse_event_at(click, &mut app, terminal_area, &mut tracker, now);

        assert_eq!(app.pane(ActivePane::Left).current_path(), &dir);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
    }

    #[test]
    fn test_double_click_directory_opens() {
        let dir = test_dir("double_click_dir");
        let sub = dir.join("subdir");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("child.txt"), "hi").unwrap();

        let mut app = App::at(dir.clone()).unwrap();
        let mut tracker = MouseTracker::new();
        let terminal_area = Rect::new(0, 0, 100, 30);
        let t0 = Instant::now();

        let click = mouse_down(10, 2, MouseButton::Left); // Row 0 (subdir)

        // First click selects
        handle_mouse_event_at(click, &mut app, terminal_area, &mut tracker, t0);
        assert_eq!(app.pane(ActivePane::Left).current_path(), &dir);

        // Second click within 200ms opens directory
        let t1 = t0 + Duration::from_millis(200);
        handle_mouse_event_at(click, &mut app, terminal_area, &mut tracker, t1);
        assert_eq!(app.pane(ActivePane::Left).current_path(), &sub);
    }

    #[test]
    fn test_double_click_file_triggers_preview() {
        let dir = test_dir("double_click_file");
        fs::write(dir.join("code.rs"), "fn main() {}").unwrap();

        let mut app = App::at(dir).unwrap();
        let mut tracker = MouseTracker::new();
        let terminal_area = Rect::new(0, 0, 100, 30);
        let t0 = Instant::now();

        let click = mouse_down(10, 2, MouseButton::Left); // Row 0 (code.rs)

        // First click
        handle_mouse_event_at(click, &mut app, terminal_area, &mut tracker, t0);
        assert_eq!(app.mode(), Mode::Normal);

        // Second click within threshold triggers preview
        let t1 = t0 + Duration::from_millis(250);
        handle_mouse_event_at(click, &mut app, terminal_area, &mut tracker, t1);
        assert_eq!(app.mode(), Mode::Preview);
        assert!(app.preview().is_active());
    }

    #[test]
    fn test_double_click_slow_does_not_open() {
        let dir = test_dir("double_click_slow");
        let sub = dir.join("subdir");
        fs::create_dir_all(&sub).unwrap();

        let mut app = App::at(dir.clone()).unwrap();
        let mut tracker = MouseTracker::new();
        let terminal_area = Rect::new(0, 0, 100, 30);
        let t0 = Instant::now();

        let click = mouse_down(10, 2, MouseButton::Left);
        handle_mouse_event_at(click, &mut app, terminal_area, &mut tracker, t0);

        // Second click after 600ms (threshold is 400ms)
        let t1 = t0 + Duration::from_millis(600);
        handle_mouse_event_at(click, &mut app, terminal_area, &mut tracker, t1);
        assert_eq!(app.pane(ActivePane::Left).current_path(), &dir);
    }

    #[test]
    fn test_right_click_selection() {
        let dir = test_dir("right_click");
        fs::write(dir.join("alpha.txt"), "a").unwrap();
        fs::write(dir.join("beta.txt"), "b").unwrap();

        let mut app = App::at(dir.clone()).unwrap();
        app.open_in(ActivePane::Right, dir).unwrap();
        let mut tracker = MouseTracker::new();
        let terminal_area = Rect::new(0, 0, 100, 30);
        let now = Instant::now();

        // Right click row 1 in right pane
        let right_click = mouse_down(70, 3, MouseButton::Right);
        handle_mouse_event_at(right_click, &mut app, terminal_area, &mut tracker, now);

        assert_eq!(app.active_pane(), ActivePane::Right);
        assert_eq!(app.pane(ActivePane::Right).selected_index(), Some(1));
    }

    #[test]
    fn test_mouse_wheel_scrolling() {
        let dir = test_dir("wheel_scroll");
        fs::write(dir.join("f1.txt"), "1").unwrap();
        fs::write(dir.join("f2.txt"), "2").unwrap();
        fs::write(dir.join("f3.txt"), "3").unwrap();

        let mut app = App::at(dir).unwrap();
        let mut tracker = MouseTracker::new();
        let terminal_area = Rect::new(0, 0, 100, 30);

        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));

        // Wheel down moves selection down
        let scroll_down = mouse_scroll_down(10, 5);
        handle_mouse_event(scroll_down, &mut app, terminal_area, &mut tracker);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));

        handle_mouse_event(scroll_down, &mut app, terminal_area, &mut tracker);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(2));

        // Wheel up moves selection up
        let scroll_up = mouse_scroll_up(10, 5);
        handle_mouse_event(scroll_up, &mut app, terminal_area, &mut tracker);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));
    }

    #[test]
    fn test_temporary_modes_block_mouse_actions() {
        let dir = test_dir("temp_modes");
        let sub = dir.join("subdir");
        fs::create_dir_all(&sub).unwrap();

        let mut app = App::at(dir.clone()).unwrap();
        let mut tracker = MouseTracker::new();
        let terminal_area = Rect::new(0, 0, 100, 30);
        let now = Instant::now();

        // Put app into Search mode
        app.handle_action(Action::StartSearch);
        assert_eq!(app.mode(), Mode::Search);

        // Click should be completely ignored in Search mode
        let click = mouse_down(10, 2, MouseButton::Left);
        handle_mouse_event_at(click, &mut app, terminal_area, &mut tracker, now);

        assert_eq!(app.mode(), Mode::Search);
        assert_eq!(app.pane(ActivePane::Left).current_path(), &dir);
    }

    #[test]
    fn test_clicks_outside_content_safely_ignored() {
        let dir = test_dir("outside_clicks");
        fs::write(dir.join("item.txt"), "1").unwrap();

        let mut app = App::at(dir).unwrap();
        let mut tracker = MouseTracker::new();
        let terminal_area = Rect::new(0, 0, 100, 30);
        let now = Instant::now();

        // Header click (row 0)
        let header_click = mouse_down(20, 0, MouseButton::Left);
        handle_mouse_event_at(header_click, &mut app, terminal_area, &mut tracker, now);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));

        // Footer click (row 29)
        let footer_click = mouse_down(20, 29, MouseButton::Left);
        handle_mouse_event_at(footer_click, &mut app, terminal_area, &mut tracker, now);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
    }

    #[test]
    fn test_mouse_tab_click_switches_tab() {
        let dir1 = test_dir("mouse_tab_1");
        let dir2 = test_dir("mouse_tab_2");
        fs::write(dir1.join("file1.txt"), "1").unwrap();
        fs::write(dir2.join("file2.txt"), "2").unwrap();

        let mut app = App::at(dir1.clone()).unwrap();
        // Add second tab
        app.open_in(ActivePane::Left, dir2.clone()).unwrap();
        app.handle_action(Action::NewTab);
        // Now tab 0: dir1, tab 1: dir2 (active)
        assert_eq!(app.pane(ActivePane::Left).tab_count(), 2);

        let mut tracker = MouseTracker::new();
        let terminal_area = Rect::new(0, 0, 100, 30);
        let layout = ScreenLayout::calculate(terminal_area);
        let pane_rect = match layout.main() {
            MainLayout::Two { left, .. } => left,
            MainLayout::Three { left, .. } => left,
            MainLayout::Single { pane } => pane,
        };

        // Find hit range for tab 0
        let ranges = crate::ui::panes::tab_hit_ranges(pane_rect, app.pane(ActivePane::Left), true);
        assert!(ranges.len() >= 2);
        let (tab0_idx, tab0_start, tab0_end) = ranges[0];
        assert_eq!(tab0_idx, 0);

        // Click on tab 0 in the pane header row (pane_rect.y)
        let click_x = (tab0_start + tab0_end) / 2;
        let tab_click = mouse_down(click_x, pane_rect.y, MouseButton::Left);
        handle_mouse_event_at(
            tab_click,
            &mut app,
            terminal_area,
            &mut tracker,
            Instant::now(),
        );

        assert_eq!(app.pane(ActivePane::Left).active_tab_index(), 0);
    }
}
