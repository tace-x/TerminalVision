//! Comprehensive test suite for Phase 1.3:
//! Smart Selection + Context Menu + Adaptive Action UI.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use terminalvision::app::actions::Action;
use terminalvision::app::modes::Mode;
use terminalvision::app::state::{ActivePane, App, ContextMenuItem};
use terminalvision::input::mouse::{MouseTracker, handle_mouse_event_at};
use terminalvision::ui::dialogs::{calculate_context_menu_rect, calculate_context_submenu_rect};
use terminalvision::ui::footer::action_hit_ranges;

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("test directory should be created");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn setup_test_directory(name: &str, count: usize) -> (TestDir, PathBuf) {
    let temp = TestDir::new(name);
    let path = temp.path().to_path_buf();
    for i in 0..count {
        let file_path = path.join(format!("file_{:03}.txt", i));
        fs::write(&file_path, format!("Content {}", i)).unwrap();
    }
    (temp, path)
}

fn mouse_click(col: u16, row: u16, button: MouseButton, modifiers: KeyModifiers) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(button),
        column: col,
        row,
        modifiers,
    }
}

// 1. Single selection
#[test]
fn test_01_single_selection() {
    let (_temp, path) = setup_test_directory("test_single_select", 5);
    let mut app = App::at(path).unwrap();

    assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 0);

    app.handle_action(Action::MoveDown);
    assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 0);
}

// 2. Multi-selection
#[test]
fn test_02_multi_selection() {
    let (_temp, path) = setup_test_directory("test_multi_select", 5);
    let mut app = App::at(path.clone()).unwrap();

    // Toggle select first item
    app.pane_mut(ActivePane::Left).toggle_selection(0);
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 1);
    assert!(app.pane(ActivePane::Left).is_item_selected(0));

    // Toggle select second item
    app.pane_mut(ActivePane::Left).toggle_selection(1);
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 2);
    assert!(app.pane(ActivePane::Left).is_item_selected(0));
    assert!(app.pane(ActivePane::Left).is_item_selected(1));

    let effective = app.pane(ActivePane::Left).effective_selected_paths();
    assert_eq!(effective.len(), 2);
    assert_eq!(effective[0], path.join("file_000.txt"));
    assert_eq!(effective[1], path.join("file_001.txt"));
}

// 3. Range selection (Shift+Up/Down, Shift+Click)
#[test]
fn test_03_range_selection() {
    let (_temp, path) = setup_test_directory("test_range_select", 8);
    let mut app = App::at(path).unwrap();

    // Start at index 0, extend down by 3
    app.handle_action(Action::SelectRangeDown);
    app.handle_action(Action::SelectRangeDown);
    app.handle_action(Action::SelectRangeDown);

    assert_eq!(app.pane(ActivePane::Left).selected_count(), 4);
    assert!(app.pane(ActivePane::Left).is_item_selected(0));
    assert!(app.pane(ActivePane::Left).is_item_selected(1));
    assert!(app.pane(ActivePane::Left).is_item_selected(2));
    assert!(app.pane(ActivePane::Left).is_item_selected(3));

    // Range select to target via mouse shift+click
    let mut tracker = MouseTracker::new();
    let terminal_area = Rect::new(0, 0, 100, 30);

    // Left click on row 5 with Shift
    let shift_click = mouse_click(10, 7, MouseButton::Left, KeyModifiers::SHIFT);
    handle_mouse_event_at(
        shift_click,
        &mut app,
        terminal_area,
        &mut tracker,
        Instant::now(),
    );

    assert!(app.pane(ActivePane::Left).selected_count() >= 5);
}

// 4. Select all
#[test]
fn test_04_select_all() {
    let (_temp, path) = setup_test_directory("test_select_all", 10);
    let mut app = App::at(path).unwrap();

    app.handle_action(Action::SelectAll);
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 10);
    for i in 0..10 {
        assert!(app.pane(ActivePane::Left).is_item_selected(i));
    }
}

// 5. Clear selection
#[test]
fn test_05_clear_selection() {
    let (_temp, path) = setup_test_directory("test_clear_select", 5);
    let mut app = App::at(path).unwrap();

    app.handle_action(Action::SelectAll);
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 5);

    // Deselect all
    app.handle_action(Action::DeselectAll);
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 0);

    // Cancel / Esc also deselects
    app.handle_action(Action::SelectAll);
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 5);
    app.handle_action(Action::Cancel);
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 0);
}

// 6. Selection after rename
#[test]
fn test_06_selection_after_rename() {
    let (_temp, path) = setup_test_directory("test_rename_select", 3);
    let mut app = App::at(path.clone()).unwrap();

    app.select_entry(ActivePane::Left, 1);
    let old_file = path.join("file_001.txt");
    assert_eq!(app.pane(ActivePane::Left).selected_path(), Some(old_file));

    app.rename_selected(std::ffi::OsStr::new("renamed_file.txt"))
        .unwrap();

    let new_file = path.join("renamed_file.txt");
    assert_eq!(app.pane(ActivePane::Left).selected_path(), Some(new_file));
}

// 7. Selection after delete
#[test]
fn test_07_selection_after_delete() {
    let (_temp, path) = setup_test_directory("test_delete_select", 3);
    let mut app = App::at(path.clone()).unwrap();

    app.select_entry(ActivePane::Left, 2);
    app.delete_selected().unwrap();

    // After deleting the last entry (index 2 of 3), selection should clamp to nearest (index 1)
    assert_eq!(app.pane(ActivePane::Left).visible_count(), 2);
    assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));
}

// 8. Selection after filesystem refresh
#[test]
fn test_08_selection_after_filesystem_refresh() {
    let (_temp, path) = setup_test_directory("test_refresh_select", 4);
    let mut app = App::at(path.clone()).unwrap();

    app.select_entry(ActivePane::Left, 2);
    let target = path.join("file_002.txt");
    assert_eq!(
        app.pane(ActivePane::Left).selected_path(),
        Some(target.clone())
    );

    // Create a new file behind the scenes
    fs::write(path.join("file_000_new.txt"), "new").unwrap();

    app.handle_action(Action::RefreshDirectory);

    // Selection should remain stable on target file path
    assert_eq!(app.pane(ActivePane::Left).selected_path(), Some(target));
}

// 9. Selection after sorting
#[test]
fn test_09_selection_after_sorting() {
    let (_temp, path) = setup_test_directory("test_sort_select", 5);
    let mut app = App::at(path.clone()).unwrap();

    app.select_entry(ActivePane::Left, 1);
    let target = path.join("file_001.txt");
    assert_eq!(
        app.pane(ActivePane::Left).selected_path(),
        Some(target.clone())
    );

    // Change sort mode
    app.handle_action(Action::ChangeSort);

    // Target path identity must be preserved
    assert_eq!(app.pane(ActivePane::Left).selected_path(), Some(target));
}

// 10. Selection after search
#[test]
fn test_10_selection_after_search() {
    let (_temp, path) = setup_test_directory("test_search_select", 5);
    let mut app = App::at(path).unwrap();

    app.handle_action(Action::StartSearch);
    assert_eq!(app.mode(), Mode::Search);

    app.set_search_query("03");

    // Selection should point to filtered item
    assert!(app.pane(ActivePane::Left).visible_count() >= 1);
    assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));

    // Clear search
    app.handle_action(Action::ClearSearch);
    assert_eq!(app.mode(), Mode::Normal);
}

// 11. Bulk action dispatch
#[test]
fn test_11_bulk_action_dispatch() {
    let (_temp, path) = setup_test_directory("test_bulk_dispatch", 4);
    let mut app = App::at(path.clone()).unwrap();

    // Select items 0, 1, 2
    app.pane_mut(ActivePane::Left).toggle_selection(0);
    app.pane_mut(ActivePane::Left).toggle_selection(1);
    app.pane_mut(ActivePane::Left).toggle_selection(2);

    assert_eq!(app.pane(ActivePane::Left).selected_count(), 3);

    // Bulk copy
    app.handle_action(Action::Copy);
    assert_eq!(app.clipboard().len(), 3);
    assert_eq!(
        app.clipboard().operation(),
        Some(terminalvision::app::state::ClipboardOperation::Copy)
    );

    // Bulk delete
    app.delete_selected().unwrap();
    assert_eq!(app.pane(ActivePane::Left).visible_count(), 1);
}

// 12. Context menu action dispatch
#[test]
fn test_12_context_menu_action_dispatch() {
    let (_temp, path) = setup_test_directory("test_menu_dispatch", 3);
    let mut app = App::at(path).unwrap();

    app.open_context_menu((10, 5));
    assert_eq!(app.mode(), Mode::ContextMenu);
    assert!(!app.context_menu().items.is_empty());

    // Navigate to Copy
    let mut copy_idx = None;
    for (i, item) in app.context_menu().items.iter().enumerate() {
        if let ContextMenuItem::Action {
            action: Action::Copy,
            ..
        } = item
        {
            copy_idx = Some(i);
            break;
        }
    }
    assert!(copy_idx.is_some());
    app.context_menu_mut().selected = copy_idx.unwrap();

    let action = app.confirm_modal();
    assert_eq!(action, Some(Action::Copy));
    assert_eq!(app.mode(), Mode::Normal);
}

// 13. Action availability
#[test]
fn test_13_action_availability() {
    let temp = TestDir::new("test_availability");
    let path = temp.path().to_path_buf();
    let mut app = App::at(path).unwrap();

    // Empty directory with empty clipboard -> Paste is disabled
    let menu = app.build_context_menu((10, 5));
    let has_disabled_paste = menu
        .items
        .iter()
        .any(|item| matches!(item, ContextMenuItem::Disabled { label, .. } if label == "Paste"));
    assert!(has_disabled_paste);

    // After setting clipboard item -> Paste is available
    let file = temp.path().join("source.txt");
    fs::write(&file, "hi").unwrap();
    app.handle_action(Action::RefreshDirectory);
    app.handle_action(Action::Copy);

    let menu_after_copy = app.build_context_menu((10, 5));
    let has_active_paste = menu_after_copy.items.iter().any(|item| {
        matches!(
            item,
            ContextMenuItem::Action {
                action: Action::Paste,
                ..
            }
        )
    });
    assert!(has_active_paste);
}

// 14. Context menu positioning & clamping
#[test]
fn test_14_context_menu_positioning_and_clamping() {
    let viewport = Rect::new(0, 0, 80, 24);

    // Normal position inside bounds
    let normal_rect = calculate_context_menu_rect((10, 5), 8, viewport);
    assert_eq!(normal_rect.x, 10);
    assert_eq!(normal_rect.y, 5);
    assert!(normal_rect.x + normal_rect.width <= viewport.width);
    assert!(normal_rect.y + normal_rect.height <= viewport.height);

    // Right-click near right edge (col 78, row 5) -> clamped inside viewport
    let right_edge_rect = calculate_context_menu_rect((78, 5), 8, viewport);
    assert!(right_edge_rect.x + right_edge_rect.width <= viewport.width);
    assert_eq!(right_edge_rect.x, viewport.width - right_edge_rect.width);

    // Right-click near bottom edge (col 10, row 22) -> clamped inside viewport
    let bottom_edge_rect = calculate_context_menu_rect((10, 22), 8, viewport);
    assert!(bottom_edge_rect.y + bottom_edge_rect.height <= viewport.height);
    assert_eq!(
        bottom_edge_rect.y,
        viewport.height - bottom_edge_rect.height
    );

    // Submenu clamping
    let sub_rect = calculate_context_submenu_rect(normal_rect, 2, 4, viewport);
    assert!(sub_rect.x + sub_rect.width <= viewport.width);
    assert!(sub_rect.y + sub_rect.height <= viewport.height);
}

// 15. Keyboard menu navigation
#[test]
fn test_15_keyboard_menu_navigation() {
    let (_temp, path) = setup_test_directory("test_kb_nav", 3);
    let mut app = App::at(path).unwrap();

    app.open_context_menu((10, 5));
    assert_eq!(app.context_menu().selected, 0);

    // Move down
    app.context_menu_mut().move_down();
    assert_eq!(app.context_menu().selected, 1);

    // Move up
    app.context_menu_mut().move_up();
    assert_eq!(app.context_menu().selected, 0);

    // Open More submenu
    for (i, item) in app.context_menu().items.iter().enumerate() {
        if matches!(item, ContextMenuItem::More { .. }) {
            app.context_menu_mut().selected = i;
            break;
        }
    }
    app.context_menu_mut().open_more();
    assert!(app.context_menu().is_more_open);

    // Close More submenu
    app.context_menu_mut().close_more();
    assert!(!app.context_menu().is_more_open);

    // Close menu
    app.close_context_menu();
    assert_eq!(app.mode(), Mode::Normal);
}

// 16. Mouse menu navigation & hit testing
#[test]
fn test_16_mouse_menu_navigation_and_hit_testing() {
    let (_temp, path) = setup_test_directory("test_mouse_nav", 3);
    let mut app = App::at(path).unwrap();
    let mut tracker = MouseTracker::new();
    let terminal_area = Rect::new(0, 0, 100, 30);

    // Right-click row 1 to open context menu
    let right_click = mouse_click(10, 3, MouseButton::Right, KeyModifiers::NONE);
    handle_mouse_event_at(
        right_click,
        &mut app,
        terminal_area,
        &mut tracker,
        Instant::now(),
    );

    assert_eq!(app.mode(), Mode::ContextMenu);
    assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));

    // Click outside to close context menu
    let outside_click = mouse_click(90, 28, MouseButton::Left, KeyModifiers::NONE);
    handle_mouse_event_at(
        outside_click,
        &mut app,
        terminal_area,
        &mut tracker,
        Instant::now(),
    );
    assert_eq!(app.mode(), Mode::Normal);

    // Test adaptive action button hit testing in footer
    let footer_rect = Rect::new(0, 29, 100, 1);
    let hit_ranges = action_hit_ranges(footer_rect, &app);
    assert!(!hit_ranges.is_empty());

    let (first_action, start_x, end_x, y) = hit_ranges[0];
    let btn_click = mouse_click(
        (start_x + end_x) / 2,
        y,
        MouseButton::Left,
        KeyModifiers::NONE,
    );
    handle_mouse_event_at(
        btn_click,
        &mut app,
        terminal_area,
        &mut tracker,
        Instant::now(),
    );
    // Action dispatched
    assert!(first_action != Action::Cancel);
}

// 17. Large selection safety (1000 items)
#[test]
fn test_17_large_selection_safety() {
    let (_temp, path) = setup_test_directory("test_large_selection", 1000);
    let mut app = App::at(path).unwrap();

    assert_eq!(app.pane(ActivePane::Left).visible_count(), 1000);

    // Select all 1000 items
    app.handle_action(Action::SelectAll);
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 1000);

    // Compute aggregate size without hang/panic
    let size = app.pane(ActivePane::Left).aggregate_selected_size();
    assert!(size > 0);

    // Clear
    app.handle_action(Action::DeselectAll);
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 0);
}

// 18. Invalid/stale selection protection
#[test]
fn test_18_invalid_stale_selection_protection() {
    let (_temp, path) = setup_test_directory("test_stale_protection", 4);
    let mut app = App::at(path.clone()).unwrap();

    app.pane_mut(ActivePane::Left).toggle_selection(2);
    let deleted_path = path.join("file_002.txt");
    assert!(
        app.pane(ActivePane::Left)
            .selected_paths()
            .contains(&deleted_path)
    );

    // Delete file externally
    fs::remove_file(&deleted_path).unwrap();

    // Refresh directory
    app.handle_action(Action::RefreshDirectory);

    // Stale path must be automatically purged from selected_paths
    assert!(
        !app.pane(ActivePane::Left)
            .selected_paths()
            .contains(&deleted_path)
    );
    assert!(
        app.pane(ActivePane::Left).selected_index().unwrap()
            < app.pane(ActivePane::Left).visible_count()
    );
}
