use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect;
use tempfile::TempDir;
use terminalvision::app::actions::Action;
use terminalvision::app::modes::Mode;
use terminalvision::app::state::{ActivePane, App};
use terminalvision::commands::context_menu::{
    ContextMenuItem, ContextMenuState, ContextMenuTarget, build_context_menu_items,
};
use terminalvision::input::keyboard::map_key_event_with_platform;
use terminalvision::input::mouse::{MouseTracker, handle_mouse_event_at};
use terminalvision::input::platform::Platform;
use terminalvision::ui::dialogs::{calculate_context_menu_rect, calculate_context_submenu_rect};

fn create_test_environment() -> (TempDir, PathBuf) {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let path = temp_dir.path().to_path_buf();

    fs::write(
        path.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(path.join("README.md"), "# Demo Project\n").unwrap();
    fs::create_dir_all(path.join("src")).unwrap();
    fs::write(path.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::create_dir_all(path.join("docs")).unwrap();
    fs::write(path.join("docs/guide.txt"), "Documentation\n").unwrap();

    (temp_dir, path)
}

fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
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
// 1. Keyboard Context Menu Invocation (Shift+F10)
// -----------------------------------------------------------------------------
#[test]
fn test_01_keyboard_context_menu_invocation_shift_f10() {
    let (_tmp, path) = create_test_environment();
    let mut app = App::at(path.clone()).unwrap();

    let key = key_event(KeyCode::F(10), KeyModifiers::SHIFT);
    let action = terminalvision::input::keyboard::map_key(key, Mode::Normal);
    assert_eq!(action, Some(Action::ContextMenu));

    app.handle_action(Action::ContextMenu);
    assert_eq!(app.mode(), Mode::ContextMenu);
    assert!(!app.context_menu().items.is_empty());
}

// -----------------------------------------------------------------------------
// 2. Keyboard Context Menu Invocation (Dedicated Menu Key)
// -----------------------------------------------------------------------------
#[test]
fn test_02_keyboard_context_menu_invocation_menu_key() {
    let (_tmp, path) = create_test_environment();
    let mut app = App::at(path.clone()).unwrap();

    let key = key_event(KeyCode::Menu, KeyModifiers::NONE);
    let action = terminalvision::input::keyboard::map_key(key, Mode::Normal);
    assert_eq!(action, Some(Action::ContextMenu));

    app.handle_action(Action::ContextMenu);
    assert_eq!(app.mode(), Mode::ContextMenu);
}

// -----------------------------------------------------------------------------
// 3. Keyboard Navigation (Up/Down with Wrapping)
// -----------------------------------------------------------------------------
#[test]
fn test_03_keyboard_navigation_up_down_wrapping() {
    let (_tmp, path) = create_test_environment();
    let mut app = App::at(path).unwrap();
    app.open_context_menu((10, 10));

    let menu = app.context_menu_mut();
    assert_eq!(menu.selected, 0);

    // Navigate down
    menu.move_down();
    assert_eq!(menu.selected, 1);

    // Jump to first, move up -> should wrap to last selectable item
    menu.jump_first();
    assert_eq!(menu.selected, 0);
    menu.move_up();
    assert!(menu.selected > 0);
    assert!(menu.items[menu.selected].is_selectable());
}

// -----------------------------------------------------------------------------
// 4. Keyboard Home / End Navigation
// -----------------------------------------------------------------------------
#[test]
fn test_04_keyboard_home_end_jumping() {
    let (_tmp, path) = create_test_environment();
    let mut app = App::at(path).unwrap();
    app.open_context_menu((10, 10));

    let menu = app.context_menu_mut();
    menu.jump_last();
    let last_idx = menu.selected;
    assert!(menu.items[last_idx].is_selectable());

    menu.jump_first();
    assert_eq!(menu.selected, 0);
}

// -----------------------------------------------------------------------------
// 5. Keyboard PageUp / PageDown Navigation
// -----------------------------------------------------------------------------
#[test]
fn test_05_keyboard_page_up_page_down() {
    let (_tmp, path) = create_test_environment();
    let mut app = App::at(path).unwrap();
    app.open_context_menu((10, 10));

    let menu = app.context_menu_mut();
    menu.jump_first();
    assert_eq!(menu.selected, 0);

    menu.page_down();
    assert!(menu.selected > 0);
    assert!(menu.items[menu.selected].is_selectable());

    menu.page_up();
    assert!(menu.items[menu.selected].is_selectable());
}

// -----------------------------------------------------------------------------
// 6. Keyboard Type-to-Select: Single Character
// -----------------------------------------------------------------------------
#[test]
fn test_06_keyboard_type_to_select_single_char() {
    let items = vec![
        ContextMenuItem::action(Action::Open, "Open", None),
        ContextMenuItem::action(Action::Preview, "Quick Preview", None),
        ContextMenuItem::separator(),
        ContextMenuItem::action(Action::Copy, "Copy", None),
        ContextMenuItem::action(Action::Cut, "Cut", None),
        ContextMenuItem::action(Action::Rename, "Rename", None),
        ContextMenuItem::action(Action::Delete, "Delete", None),
    ];
    let mut state = ContextMenuState::new(None, items, (10, 10));

    // Type 'r' -> should jump directly to "Rename"
    state.type_to_select('r');
    assert_eq!(state.items[state.selected].label(), "Rename");

    // Type 'd' -> should jump directly to "Delete"
    state.type_to_select('d');
    assert_eq!(state.items[state.selected].label(), "Delete");
}

// -----------------------------------------------------------------------------
// 7. Keyboard Type-to-Select: Multi-Character Prefix
// -----------------------------------------------------------------------------
#[test]
fn test_07_keyboard_type_to_select_multi_char_prefix() {
    let items = vec![
        ContextMenuItem::action(Action::Copy, "Copy", None),
        ContextMenuItem::action(Action::CopyPath, "Copy Path", None),
        ContextMenuItem::action(Action::CopyName, "Copy Name", None),
        ContextMenuItem::action(Action::Delete, "Delete", None),
        ContextMenuItem::action(Action::Rename, "Rename", None),
    ];
    let mut state = ContextMenuState::new(None, items, (10, 10));

    // Type 'c', 'o', 'p', 'y', ' ', 'p' -> "Copy Path"
    state.type_to_select('c');
    state.type_to_select('o');
    state.type_to_select('p');
    state.type_to_select('y');
    state.type_to_select(' ');
    state.type_to_select('p');

    assert_eq!(state.items[state.selected].label(), "Copy Path");
}

// -----------------------------------------------------------------------------
// 8. Keyboard Type-to-Select: Repeated Same Character Cycling
// -----------------------------------------------------------------------------
#[test]
fn test_08_keyboard_type_to_select_cycling_same_char() {
    let items = vec![
        ContextMenuItem::action(Action::Copy, "Copy", None),
        ContextMenuItem::action(Action::Cut, "Cut", None),
        ContextMenuItem::action(Action::Delete, "Delete", None),
    ];
    let mut state = ContextMenuState::new(None, items, (10, 10));

    // Start at Delete (idx 2)
    state.select_index(2);
    assert_eq!(state.items[state.selected].label(), "Delete");

    // First 'c' -> jumps to Copy (idx 0)
    state.type_to_select('c');
    assert_eq!(state.items[state.selected].label(), "Copy");

    // Next 'c' -> cycles to Cut (idx 1)
    state.last_type_time = None;
    state.type_to_select('c');
    assert_eq!(state.items[state.selected].label(), "Cut");

    // Next 'c' -> cycles back to Copy (idx 0)
    state.last_type_time = None;
    state.type_to_select('c');
    assert_eq!(state.items[state.selected].label(), "Copy");
}

// -----------------------------------------------------------------------------
// 9. Submenu Keyboard Expansion (Right Arrow / Enter)
// -----------------------------------------------------------------------------
#[test]
fn test_09_submenu_keyboard_expansion_right_arrow() {
    let (_tmp, path) = create_test_environment();
    let mut app = App::at(path).unwrap();
    app.open_context_menu((10, 10));

    let more_idx = app
        .context_menu()
        .items
        .iter()
        .position(|i| matches!(i, ContextMenuItem::More { .. }))
        .expect("More submenu item must exist");

    app.context_menu_mut().select_index(more_idx);
    assert_eq!(app.context_menu().selected, more_idx);
    assert!(!app.context_menu().is_more_open);

    // Expand submenu via open_more
    app.context_menu_mut().open_more();
    assert!(app.context_menu().is_more_open);
    assert_eq!(app.context_menu().more_selected, 0);
}

// -----------------------------------------------------------------------------
// 10. Submenu Keyboard Collapse (Left Arrow / Esc)
// -----------------------------------------------------------------------------
#[test]
fn test_10_submenu_keyboard_collapse_left_arrow() {
    let (_tmp, path) = create_test_environment();
    let mut app = App::at(path).unwrap();
    app.open_context_menu((10, 10));

    let more_idx = app
        .context_menu()
        .items
        .iter()
        .position(|i| matches!(i, ContextMenuItem::More { .. }))
        .unwrap();

    app.context_menu_mut().select_index(more_idx);
    app.context_menu_mut().open_more();
    assert!(app.context_menu().is_more_open);

    // Left arrow / close_more collapses submenu
    app.context_menu_mut().close_more();
    assert!(!app.context_menu().is_more_open);
    assert_eq!(app.mode(), Mode::ContextMenu); // Menu remains open!

    // Second cancel closes the context menu
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);
}

// -----------------------------------------------------------------------------
// 11. Submenu Position Reversal Near Right Edge
// -----------------------------------------------------------------------------
#[test]
fn test_11_submenu_reversal_near_right_edge() {
    let terminal_area = Rect::new(0, 0, 80, 24);
    // Menu placed near the right edge: x = 60, width = 30 -> overflows 80
    let menu_rect = calculate_context_menu_rect((60, 5), 10, terminal_area);

    let sub_rect = calculate_context_submenu_rect(menu_rect, 2, 5, terminal_area);

    // Submenu must be placed to the left of menu_rect to prevent clipping
    assert!(sub_rect.x + sub_rect.width <= terminal_area.width);
    assert!(sub_rect.x < menu_rect.x);
}

// -----------------------------------------------------------------------------
// 12. Disabled Actions Cannot Be Activated
// -----------------------------------------------------------------------------
#[test]
fn test_12_disabled_actions_non_activatable() {
    let items = vec![
        ContextMenuItem::action(Action::Copy, "Copy", None),
        ContextMenuItem::disabled("Paste", Some("Clipboard empty".to_string())),
        ContextMenuItem::action(Action::Delete, "Delete", None),
    ];
    let mut state = ContextMenuState::new(None, items, (10, 10));

    // Move down from Copy -> should jump directly to Delete, skipping Disabled Paste!
    state.select_index(0);
    state.move_down();
    assert_eq!(state.selected, 2);
    assert_eq!(state.items[state.selected].label(), "Delete");
}

// -----------------------------------------------------------------------------
// 13. Dynamic Shortcuts from Centralized Shortcut Registry
// -----------------------------------------------------------------------------
#[test]
fn test_13_dynamic_shortcuts_from_registry() {
    let target = ContextMenuTarget::File {
        path: PathBuf::from("demo.rs"),
        is_executable: false,
    };
    let items = build_context_menu_items(&target, true, Platform::Mac);

    // Verify Copy has "⌘C" on Mac
    let copy_item = items
        .iter()
        .find(|i| i.action_target() == Some(Action::Copy))
        .unwrap();
    if let ContextMenuItem::Action { shortcut, .. } = copy_item {
        assert_eq!(shortcut.as_deref(), Some("⌘C"));
    }

    // Verify Copy has "Ctrl+C" on Linux
    let linux_items = build_context_menu_items(&target, true, Platform::Linux);
    let linux_copy = linux_items
        .iter()
        .find(|i| i.action_target() == Some(Action::Copy))
        .unwrap();
    if let ContextMenuItem::Action { shortcut, .. } = linux_copy {
        assert_eq!(shortcut.as_deref(), Some("Ctrl+C"));
    }
}

// -----------------------------------------------------------------------------
// 14. Mouse Hover and Click Parity with Keyboard
// -----------------------------------------------------------------------------
#[test]
fn test_14_mouse_hover_and_click_parity_with_keyboard() {
    let (_tmp, path) = create_test_environment();
    let mut app = App::at(path).unwrap();
    let terminal_area = Rect::new(0, 0, 100, 30);
    let mut tracker = MouseTracker::new();

    app.open_context_menu((10, 5));
    let menu_rect =
        calculate_context_menu_rect((10, 5), app.context_menu().items.len(), terminal_area);

    // Find Rename item
    let rename_idx = app
        .context_menu()
        .items
        .iter()
        .position(|i| i.action_target() == Some(Action::Rename))
        .unwrap();

    let click_y = menu_rect.y + 1 + rename_idx as u16;
    let click_x = menu_rect.x + 5;

    let click_event = mouse_event(MouseEventKind::Down(MouseButton::Left), click_x, click_y);
    handle_mouse_event_at(
        click_event,
        &mut app,
        terminal_area,
        &mut tracker,
        Instant::now(),
    );

    // Context menu executed Action::Rename and transitioned to Mode::Rename!
    assert_eq!(app.mode(), Mode::Rename);
}

// -----------------------------------------------------------------------------
// 15. Multi-Selection Count Aware Keyboard and Mouse Dispatch
// -----------------------------------------------------------------------------
#[test]
fn test_15_multi_selection_count_aware_keyboard_and_mouse() {
    let target = ContextMenuTarget::MultiSelection {
        count: 5,
        paths: vec![
            PathBuf::from("a.txt"),
            PathBuf::from("b.txt"),
            PathBuf::from("c.txt"),
            PathBuf::from("d.txt"),
            PathBuf::from("e.txt"),
        ],
    };
    let items = build_context_menu_items(&target, false, Platform::Mac);

    let copy_item = items
        .iter()
        .find(|i| i.action_target() == Some(Action::Copy))
        .unwrap();
    assert_eq!(copy_item.label(), "Copy 5 Items");

    let del_item = items
        .iter()
        .find(|i| i.action_target() == Some(Action::Delete))
        .unwrap();
    assert_eq!(del_item.label(), "Delete 5 Items");
}

// -----------------------------------------------------------------------------
// 16. Terminal Focus Isolation and Keystroke Safety
// -----------------------------------------------------------------------------
#[test]
fn test_16_terminal_focus_isolation_and_keystroke_safety() {
    let (_tmp, path) = create_test_environment();
    let mut app = App::at(path).unwrap();

    // Focus embedded terminal
    app.handle_action(Action::FocusTerminal);
    assert_eq!(app.mode(), Mode::Terminal);

    // Regular character key event must pass through to terminal PTY as TerminalKey
    let key = key_event(KeyCode::Char('l'), KeyModifiers::NONE);
    let mapped = map_key_event_with_platform(key, Mode::Terminal, Platform::Mac);
    assert!(matches!(
        mapped,
        terminalvision::input::InputEvent::TerminalKey(_)
    ));

    // Shift+F10 in terminal mode does NOT open FM context menu
    let f10_key = key_event(KeyCode::F(10), KeyModifiers::SHIFT);
    let mapped_f10 = map_key_event_with_platform(f10_key, Mode::Terminal, Platform::Mac);
    assert_ne!(
        mapped_f10,
        terminalvision::input::InputEvent::Action(Action::ContextMenu)
    );
}

// -----------------------------------------------------------------------------
// 17. Focus Restoration After Context Menu Close
// -----------------------------------------------------------------------------
#[test]
fn test_17_focus_restoration_after_context_menu_close() {
    let (_tmp, path) = create_test_environment();
    let mut app = App::at(path).unwrap();
    assert_eq!(app.mode(), Mode::Normal);
    assert_eq!(app.active_pane(), ActivePane::Left);

    app.open_context_menu((10, 10));
    assert_eq!(app.mode(), Mode::ContextMenu);

    // Cancel / Esc restores Mode::Normal with left pane active
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);
    assert_eq!(app.active_pane(), ActivePane::Left);
}

// -----------------------------------------------------------------------------
// 18. Menu Scrolling and Selected Item Visibility
// -----------------------------------------------------------------------------
#[test]
fn test_18_menu_scrolling_and_selected_item_visibility() {
    let mut items = Vec::new();
    for i in 0..30 {
        items.push(ContextMenuItem::action(
            Action::Open,
            format!("Action {i}"),
            None,
        ));
    }
    let mut state = ContextMenuState::new(None, items, (10, 10));

    // Jump to item 25
    state.select_index(25);
    assert_eq!(state.selected, 25);

    // Test page up
    state.page_up();
    assert_eq!(state.selected, 20);

    // Test page down
    state.page_down();
    assert_eq!(state.selected, 25);
}

// -----------------------------------------------------------------------------
// 19. Responsive Geometry Across Standard Resolutions
// -----------------------------------------------------------------------------
#[test]
fn test_19_responsive_geometry_across_standard_resolutions() {
    for (width, height) in [
        (80, 24),
        (100, 30),
        (120, 30),
        (159, 30),
        (160, 40),
        (180, 40),
        (200, 60),
        (40, 12),
    ] {
        let area = Rect::new(0, 0, width, height);
        let menu_rect = calculate_context_menu_rect((width - 5, height - 2), 12, area);

        assert!(menu_rect.x + menu_rect.width <= area.width);
        assert!(menu_rect.y + menu_rect.height <= area.height);
        assert!(menu_rect.width > 0);
        assert!(menu_rect.height > 0);
    }
}

// -----------------------------------------------------------------------------
// 20. Motion Mode Parity and Immediate Dismissal
// -----------------------------------------------------------------------------
#[test]
fn test_20_motion_mode_parity_and_immediate_dismissal() {
    let (_tmp, path) = create_test_environment();
    let mut app = App::at(path).unwrap();

    // Disable motion
    app.set_reduced_motion(true);
    app.open_context_menu((10, 10));
    assert_eq!(app.mode(), Mode::ContextMenu);

    // Instant close without animation block
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);
}
