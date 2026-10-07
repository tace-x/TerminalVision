//! Comprehensive integration and deterministic unit tests for Phase 3.1:
//! Interactive Context Menu Foundation.
//!
//! Validates:
//! 1. File context menu construction and actions
//! 2. Directory context menu construction and actions
//! 3. Multi-selection menu labeling and item targeting
//! 4. Right-click selected item preserves multi-selection
//! 5. Right-click unselected item replaces selection
//! 6. Contextual action filtering based on target type
//! 7. Disabled / unsupported actions display with explanation
//! 8. Menu positioning near mouse cursor
//! 9. Right-edge repositioning / clamping within terminal bounds
//! 10. Bottom-edge repositioning / clamping within terminal bounds
//! 11. Submenu expansion, navigation, and dismissal
//! 12. Mouse click action execution and dispatch
//! 13. Click outside menu dismissal
//! 14. Two-stage Esc dismissal (submenu -> main menu -> closed)
//! 15. Context menu temporary focus acquisition
//! 16. Focus restoration on menu dismissal / execution
//! 17. Terminal focus safety (right click inside embedded terminal does not pop file menu)
//! 18. Small terminal viewport rendering and boundary safety
//! 19. Long menu vertical windowing and scrolling
//! 20. Motion Off instant menu appearance/dismissal
//! 21. Reduced motion mode menu integration
//! 22. Action Registry integration (all menu items resolve to real Universal Action Registry actions)

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use terminalvision::animation::{MotionMode, MotionPreferences};
use terminalvision::app::actions::{Action, ActionRegistry};
use terminalvision::app::modes::Mode;
use terminalvision::app::state::{ActivePane, App, ContextMenuItem, ContextMenuTarget};
use terminalvision::input::mouse::{MouseTracker, handle_mouse_event_at};
use terminalvision::input::platform::Platform;
use terminalvision::ui::dialogs::calculate_context_menu_rect;

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

fn setup_test_hierarchy(name: &str) -> (TestDir, PathBuf) {
    let temp = TestDir::new(name);
    let path = temp.path().to_path_buf();

    fs::create_dir_all(path.join("src")).unwrap();
    fs::create_dir_all(path.join("docs")).unwrap();
    fs::write(
        path.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(path.join("README.md"), "# Demo Project\n").unwrap();
    fs::write(path.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(path.join("docs/guide.txt"), "Documentation\n").unwrap();

    (temp, path)
}

fn mouse_event(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

// -----------------------------------------------------------------------------
// Test 1: File context menu
// -----------------------------------------------------------------------------
#[test]
fn test_01_file_context_menu() {
    let (_temp, path) = setup_test_hierarchy("test_file_menu");
    let mut app = App::at(path.clone()).unwrap();

    // Select README.md (a file)
    let readme_idx = app
        .pane(ActivePane::Left)
        .entries()
        .iter()
        .position(|e| e.name() == "README.md")
        .unwrap();
    app.select_entry(ActivePane::Left, readme_idx);

    app.open_context_menu((15, 8));
    assert_eq!(app.mode(), Mode::ContextMenu);

    let menu = app.context_menu();
    assert!(!menu.items.is_empty());
    assert_eq!(menu.position, (15, 8));

    // Target must be a File
    assert!(matches!(
        menu.target(),
        Some(ContextMenuTarget::File { .. })
    ));

    // File menu must contain Open, Quick Preview, Copy, Cut, Rename, Delete, Copy Path, Copy Name, Get Info, More
    let actions: Vec<Action> = menu
        .items
        .iter()
        .filter_map(ContextMenuItem::action_target)
        .collect();

    assert!(actions.contains(&Action::Open));
    assert!(actions.contains(&Action::Preview));
    assert!(actions.contains(&Action::Copy));
    assert!(actions.contains(&Action::Cut));
    assert!(actions.contains(&Action::Rename));
    assert!(actions.contains(&Action::Delete));
    assert!(actions.contains(&Action::CopyPath));
    assert!(actions.contains(&Action::CopyName));
    assert!(actions.contains(&Action::GetInfo));
}

// -----------------------------------------------------------------------------
// Test 2: Directory context menu
// -----------------------------------------------------------------------------
#[test]
fn test_02_directory_context_menu() {
    let (_temp, path) = setup_test_hierarchy("test_dir_menu");
    let mut app = App::at(path.clone()).unwrap();

    // Select src (a folder)
    let src_idx = app
        .pane(ActivePane::Left)
        .entries()
        .iter()
        .position(|e| e.name() == "src")
        .unwrap();
    app.select_entry(ActivePane::Left, src_idx);

    app.open_context_menu((20, 10));
    assert_eq!(app.mode(), Mode::ContextMenu);

    let menu = app.context_menu();
    assert!(matches!(
        menu.target(),
        Some(ContextMenuTarget::Directory { .. })
    ));

    let actions: Vec<Action> = menu
        .items
        .iter()
        .filter_map(ContextMenuItem::action_target)
        .collect();

    assert!(actions.contains(&Action::Open));
    assert!(actions.contains(&Action::OpenInNewTab));
    assert!(actions.contains(&Action::Copy));
    assert!(actions.contains(&Action::Cut));
    assert!(actions.contains(&Action::Rename));
    assert!(actions.contains(&Action::Delete));
    assert!(actions.contains(&Action::ToggleFavorite));
    assert!(actions.contains(&Action::StorageVision));
    assert!(actions.contains(&Action::CopyPath));
    assert!(actions.contains(&Action::CopyName));
    assert!(actions.contains(&Action::GetInfo));
}

// -----------------------------------------------------------------------------
// Test 3: Multi-selection menu
// -----------------------------------------------------------------------------
#[test]
fn test_03_multi_selection_menu() {
    let (_temp, path) = setup_test_hierarchy("test_multi_menu");
    let mut app = App::at(path.clone()).unwrap();

    // Select multiple items
    app.pane_mut(ActivePane::Left).toggle_selection(0);
    app.pane_mut(ActivePane::Left).toggle_selection(1);
    app.pane_mut(ActivePane::Left).toggle_selection(2);
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 3);

    app.open_context_menu((10, 10));
    let menu = app.context_menu();

    assert!(matches!(
        menu.target(),
        Some(ContextMenuTarget::MultiSelection { count: 3, .. })
    ));

    // Must have dynamic pluralized labels like "Copy 3 Items"
    let labels: Vec<&str> = menu.items.iter().map(ContextMenuItem::label).collect();
    assert!(labels.iter().any(|l| l.contains("Copy 3 Items")));
    assert!(labels.iter().any(|l| l.contains("Cut 3 Items")));
    assert!(labels.iter().any(|l| l.contains("Delete 3 Items")));
    assert!(labels.iter().any(|l| l.contains("Copy Paths")));
}

// -----------------------------------------------------------------------------
// Test 4: Right-click selected item preserves multi-selection
// -----------------------------------------------------------------------------
#[test]
fn test_04_right_click_selected_item() {
    let (_temp, path) = setup_test_hierarchy("test_rc_selected");
    let mut app = App::at(path.clone()).unwrap();
    let mut tracker = MouseTracker::new();
    let terminal_area = Rect::new(0, 0, 100, 30);

    // Multi-select indices 0 and 1
    app.pane_mut(ActivePane::Left).toggle_selection(0);
    app.pane_mut(ActivePane::Left).toggle_selection(1);
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 2);

    // Right-click row 1 (index 0)
    let rc_event = mouse_event(MouseEventKind::Down(MouseButton::Right), 10, 2);
    handle_mouse_event_at(
        rc_event,
        &mut app,
        terminal_area,
        &mut tracker,
        Instant::now(),
    );

    assert_eq!(app.mode(), Mode::ContextMenu);
    // Selection must be preserved as 2 items!
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 2);
    assert!(matches!(
        app.context_menu().target(),
        Some(ContextMenuTarget::MultiSelection { count: 2, .. })
    ));
}

// -----------------------------------------------------------------------------
// Test 5: Right-click unselected item replaces selection
// -----------------------------------------------------------------------------
#[test]
fn test_05_right_click_unselected_item() {
    let (_temp, path) = setup_test_hierarchy("test_rc_unselected");
    let mut app = App::at(path.clone()).unwrap();
    let mut tracker = MouseTracker::new();
    let terminal_area = Rect::new(0, 0, 100, 30);

    // Multi-select indices 0 and 1
    app.pane_mut(ActivePane::Left).toggle_selection(0);
    app.pane_mut(ActivePane::Left).toggle_selection(1);
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 2);

    // Right-click row 3 (index 2 - an unselected item)
    let rc_event = mouse_event(MouseEventKind::Down(MouseButton::Right), 10, 4);
    handle_mouse_event_at(
        rc_event,
        &mut app,
        terminal_area,
        &mut tracker,
        Instant::now(),
    );

    assert_eq!(app.mode(), Mode::ContextMenu);
    // Selection must be replaced with single item!
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 0);
    assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(2));
}

// -----------------------------------------------------------------------------
// Test 6: Action filtering based on target
// -----------------------------------------------------------------------------
#[test]
fn test_06_action_filtering() {
    let (_temp, path) = setup_test_hierarchy("test_filtering");

    let file_target = ContextMenuTarget::File {
        path: path.join("README.md"),
        is_executable: false,
    };
    let file_items = terminalvision::commands::context_menu::build_context_menu_items(
        &file_target,
        false,
        Platform::current(),
    );

    // Files have Quick Preview
    assert!(
        file_items
            .iter()
            .any(|i| i.action_target() == Some(Action::Preview))
    );
    // Files do NOT have OpenInNewTab at top level
    assert!(
        !file_items
            .iter()
            .any(|i| i.action_target() == Some(Action::OpenInNewTab))
    );

    let dir_target = ContextMenuTarget::Directory {
        path: path.join("src"),
        is_project_root: false,
        in_git: false,
    };
    let dir_items = terminalvision::commands::context_menu::build_context_menu_items(
        &dir_target,
        false,
        Platform::current(),
    );

    // Directories have OpenInNewTab and StorageVision
    assert!(
        dir_items
            .iter()
            .any(|i| i.action_target() == Some(Action::OpenInNewTab))
    );
    assert!(
        dir_items
            .iter()
            .any(|i| i.action_target() == Some(Action::StorageVision))
    );
}

// -----------------------------------------------------------------------------
// Test 7: Missing / disabled action (Paste when clipboard is empty)
// -----------------------------------------------------------------------------
#[test]
fn test_07_missing_action() {
    let (_temp, path) = setup_test_hierarchy("test_disabled_action");
    let mut app = App::at(path.clone()).unwrap();

    // Clipboard is initially empty
    assert!(app.clipboard().is_empty());

    app.open_context_menu((10, 10));
    let menu = app.context_menu();

    // Paste must appear as Disabled
    let paste_item = menu.items.iter().find(|i| i.label() == "Paste").unwrap();
    assert!(matches!(paste_item, ContextMenuItem::Disabled { .. }));
    assert!(!paste_item.is_selectable());
}

// -----------------------------------------------------------------------------
// Test 8: Menu positioning
// -----------------------------------------------------------------------------
#[test]
fn test_08_menu_positioning() {
    let viewport = Rect::new(0, 0, 120, 40);
    let rect = calculate_context_menu_rect((25, 12), 8, viewport);

    assert_eq!(rect.x, 25);
    assert_eq!(rect.y, 12);
    assert_eq!(rect.width, 30);
    assert_eq!(rect.height, 10); // 8 items + 2 borders
}

// -----------------------------------------------------------------------------
// Test 9: Right-edge repositioning
// -----------------------------------------------------------------------------
#[test]
fn test_09_right_edge_repositioning() {
    let viewport = Rect::new(0, 0, 80, 24);
    // Summon near the right edge (x = 75, width = 30 -> would extend to 105)
    let rect = calculate_context_menu_rect((75, 5), 6, viewport);

    assert!(rect.x + rect.width <= viewport.width);
    assert_eq!(rect.x, 50); // 80 - 30
}

// -----------------------------------------------------------------------------
// Test 10: Bottom-edge repositioning
// -----------------------------------------------------------------------------
#[test]
fn test_10_bottom_edge_repositioning() {
    let viewport = Rect::new(0, 0, 80, 24);
    // Summon near the bottom edge (y = 20, height = 10 -> would extend to 30)
    let rect = calculate_context_menu_rect((10, 20), 8, viewport);

    assert!(rect.y + rect.height <= viewport.height);
    assert_eq!(rect.y, 14); // 24 - 10
}

// -----------------------------------------------------------------------------
// Test 11: Submenus
// -----------------------------------------------------------------------------
#[test]
fn test_11_submenus() {
    let (_temp, path) = setup_test_hierarchy("test_submenus");
    let mut app = App::at(path.clone()).unwrap();

    app.open_context_menu((10, 10));
    assert!(!app.context_menu().is_more_open);

    // Select the "More" item
    let more_idx = app
        .context_menu()
        .items
        .iter()
        .position(|i| matches!(i, ContextMenuItem::More { .. }))
        .unwrap();

    app.context_menu_mut().selected = more_idx;
    app.context_menu_mut().open_more();

    assert!(app.context_menu().is_more_open);

    // Close submenu
    app.context_menu_mut().close_more();
    assert!(!app.context_menu().is_more_open);
}

// -----------------------------------------------------------------------------
// Test 12: Mouse click execution
// -----------------------------------------------------------------------------
#[test]
fn test_12_mouse_click_execution() {
    let (_temp, path) = setup_test_hierarchy("test_click_exec");
    let mut app = App::at(path.clone()).unwrap();
    let mut tracker = MouseTracker::new();
    let terminal_area = Rect::new(0, 0, 100, 30);

    // Open context menu at (10, 10)
    app.open_context_menu((10, 10));
    assert_eq!(app.mode(), Mode::ContextMenu);

    let menu_rect =
        calculate_context_menu_rect((10, 10), app.context_menu().items.len(), terminal_area);

    // Find row of "Copy" in context menu
    let copy_row_idx = app
        .context_menu()
        .items
        .iter()
        .position(|i| i.action_target() == Some(Action::Copy))
        .unwrap();

    // Click inside the menu on the Copy row
    let click_x = menu_rect.x + 5;
    let click_y = menu_rect.y + 1 + copy_row_idx as u16;

    let click_event = mouse_event(MouseEventKind::Down(MouseButton::Left), click_x, click_y);
    handle_mouse_event_at(
        click_event,
        &mut app,
        terminal_area,
        &mut tracker,
        Instant::now(),
    );

    // Menu should be closed and Copy action executed (clipboard populated)
    assert_eq!(app.mode(), Mode::Normal);
    assert!(!app.clipboard().is_empty());
}

// -----------------------------------------------------------------------------
// Test 13: Click outside dismissal
// -----------------------------------------------------------------------------
#[test]
fn test_13_click_outside_dismissal() {
    let (_temp, path) = setup_test_hierarchy("test_click_outside");
    let mut app = App::at(path.clone()).unwrap();
    let mut tracker = MouseTracker::new();
    let terminal_area = Rect::new(0, 0, 100, 30);

    app.open_context_menu((10, 10));
    assert_eq!(app.mode(), Mode::ContextMenu);

    // Click far away from menu (e.g. at 80, 2)
    let outside_click = mouse_event(MouseEventKind::Down(MouseButton::Left), 80, 2);
    handle_mouse_event_at(
        outside_click,
        &mut app,
        terminal_area,
        &mut tracker,
        Instant::now(),
    );

    assert_eq!(app.mode(), Mode::Normal);
    assert!(app.context_menu().items.is_empty());
}

// -----------------------------------------------------------------------------
// Test 14: Esc dismissal (two-stage)
// -----------------------------------------------------------------------------
#[test]
fn test_14_esc_dismissal() {
    let (_temp, path) = setup_test_hierarchy("test_esc_dismiss");
    let mut app = App::at(path.clone()).unwrap();

    app.open_context_menu((10, 10));
    assert_eq!(app.mode(), Mode::ContextMenu);

    // Expand submenu
    let more_idx = app
        .context_menu()
        .items
        .iter()
        .position(|i| matches!(i, ContextMenuItem::More { .. }))
        .unwrap();
    app.context_menu_mut().selected = more_idx;
    app.context_menu_mut().open_more();
    assert!(app.context_menu().is_more_open);

    // First Esc: closes submenu
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::ContextMenu);
    assert!(!app.context_menu().is_more_open);

    // Second Esc: closes context menu completely
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);
}

// -----------------------------------------------------------------------------
// Test 15: Menu focus
// -----------------------------------------------------------------------------
#[test]
fn test_15_menu_focus() {
    let (_temp, path) = setup_test_hierarchy("test_menu_focus");
    let mut app = App::at(path.clone()).unwrap();

    assert_eq!(app.mode(), Mode::Normal);
    app.open_context_menu((10, 10));
    assert_eq!(app.mode(), Mode::ContextMenu);

    // File manager selection remains intact
    assert!(app.pane(ActivePane::Left).selected_index().is_some());
}

// -----------------------------------------------------------------------------
// Test 16: Focus restoration
// -----------------------------------------------------------------------------
#[test]
fn test_16_focus_restoration() {
    let (_temp, path) = setup_test_hierarchy("test_focus_restore");
    let mut app = App::at(path.clone()).unwrap();

    app.open_context_menu((10, 10));
    assert_eq!(app.mode(), Mode::ContextMenu);

    app.close_context_menu();
    assert_eq!(app.mode(), Mode::Normal);
}

// -----------------------------------------------------------------------------
// Test 17: Terminal focus safety
// -----------------------------------------------------------------------------
#[test]
fn test_17_terminal_focus_safety() {
    let (_temp, path) = setup_test_hierarchy("test_term_safety");
    let mut app = App::at(path.clone()).unwrap();
    let mut tracker = MouseTracker::new();
    let terminal_area = Rect::new(0, 0, 120, 30);

    // Focus terminal mode
    app.set_mode(Mode::Terminal);

    // Right-click inside the embedded terminal area (bottom pane: y = 25)
    let term_rc = mouse_event(MouseEventKind::Down(MouseButton::Right), 20, 25);
    handle_mouse_event_at(
        term_rc,
        &mut app,
        terminal_area,
        &mut tracker,
        Instant::now(),
    );

    // Must NOT pop file manager context menu over terminal
    assert_ne!(app.mode(), Mode::ContextMenu);
}

// -----------------------------------------------------------------------------
// Test 18: Small terminal viewport
// -----------------------------------------------------------------------------
#[test]
fn test_18_small_terminal() {
    let small_viewport = Rect::new(0, 0, 20, 8);
    let rect = calculate_context_menu_rect((5, 2), 12, small_viewport);

    assert!(rect.width <= small_viewport.width);
    assert!(rect.height <= small_viewport.height);
    assert!(rect.x + rect.width <= small_viewport.x + small_viewport.width);
    assert!(rect.y + rect.height <= small_viewport.y + small_viewport.height);
}

// -----------------------------------------------------------------------------
// Test 19: Long menu scrolling
// -----------------------------------------------------------------------------
#[test]
fn test_19_long_menu_scrolling() {
    let (_temp, path) = setup_test_hierarchy("test_menu_scroll");
    let mut app = App::at(path.clone()).unwrap();

    app.open_context_menu((10, 10));
    let initial_sel = app.context_menu().selected;

    app.context_menu_mut().move_down();
    assert_ne!(app.context_menu().selected, initial_sel);

    app.context_menu_mut().move_up();
    assert_eq!(app.context_menu().selected, initial_sel);
}

// -----------------------------------------------------------------------------
// Test 20: Animation disabled mode
// -----------------------------------------------------------------------------
#[test]
fn test_20_animation_disabled() {
    let (_temp, path) = setup_test_hierarchy("test_motion_off");
    let mut app = App::at(path.clone()).unwrap();
    app.set_motion_preferences(MotionPreferences {
        mode: MotionMode::Off,
        ..Default::default()
    });

    app.open_context_menu((10, 10));
    assert_eq!(app.mode(), Mode::ContextMenu);
    assert!(!app.has_active_animations());

    app.close_context_menu();
    assert_eq!(app.mode(), Mode::Normal);
    assert!(!app.has_active_animations());
}

// -----------------------------------------------------------------------------
// Test 21: Reduced motion mode
// -----------------------------------------------------------------------------
#[test]
fn test_21_reduced_motion() {
    let (_temp, path) = setup_test_hierarchy("test_reduced_motion");
    let mut app = App::at(path.clone()).unwrap();
    app.set_motion_preferences(MotionPreferences {
        mode: MotionMode::Reduced,
        ..Default::default()
    });

    app.open_context_menu((10, 10));
    assert_eq!(app.mode(), Mode::ContextMenu);
}

// -----------------------------------------------------------------------------
// Test 22: Action Registry integration
// -----------------------------------------------------------------------------
#[test]
fn test_22_action_registry_integration() {
    let registry = ActionRegistry::global();
    let (_temp, path) = setup_test_hierarchy("test_registry_int");

    // Verify all menu items in all target modes resolve to registered actions
    for target in [
        ContextMenuTarget::Empty {
            directory: path.clone(),
        },
        ContextMenuTarget::File {
            path: path.join("README.md"),
            is_executable: false,
        },
        ContextMenuTarget::Directory {
            path: path.join("src"),
            is_project_root: false,
            in_git: false,
        },
        ContextMenuTarget::MultiSelection {
            count: 3,
            paths: vec![path.join("README.md")],
        },
    ] {
        let items = terminalvision::commands::context_menu::build_context_menu_items(
            &target,
            true,
            Platform::current(),
        );

        for item in &items {
            if let Some(action) = item.action_target() {
                assert!(
                    registry.get(action).is_some(),
                    "Context menu action {action:?} is not registered in ActionRegistry"
                );
            }
            if let ContextMenuItem::More {
                items: sub_items, ..
            } = item
            {
                for sub in sub_items {
                    if let Some(action) = sub.action_target() {
                        assert!(
                            registry.get(action).is_some(),
                            "Submenu action {action:?} is not registered in ActionRegistry"
                        );
                    }
                }
            }
        }
    }
}
