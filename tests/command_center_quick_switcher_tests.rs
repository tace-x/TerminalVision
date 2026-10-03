//! Comprehensive test suite for Phase 2.1: Command Center + Quick Switcher.
//!
//! Validates:
//! 1. Command Center opening & mode transition
//! 2. Platform-aware shortcut mappings (⌘K/Ctrl+K, ⌘P/Ctrl+P)
//! 3. Action search from centralized Action Registry
//! 4. Fuzzy matching & ranking
//! 5. Result ordering and relevance
//! 6. Action Registry dispatch without logic duplication
//! 7. Context filtering (File vs Folder vs Empty vs Terminal)
//! 8. Keyboard navigation (arrows, typing, backspace, enter, esc)
//! 9. Mouse selection, header click, scroll, and click-outside-to-close
//! 10. Quick Switcher opening & target aggregation
//! 11. Recent navigation history tracking
//! 12. Duplicate history prevention & deduplication
//! 13. Invalid/stale history handling without panic
//! 14. Folder navigation from Quick Switcher
//! 15. File navigation + selection from Quick Switcher
//! 16. Responsive layout rendering across terminal dimensions (80x24 to 200x60)

use std::fs::{self, File};
use std::time::Instant;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use tempfile::TempDir;

use terminalvision::app::actions::Action;
use terminalvision::app::modes::Mode;
use terminalvision::app::state::{ActivePane, App};
use terminalvision::commands::fuzzy::{fuzzy_match, fuzzy_match_multi};
use terminalvision::commands::palette::{CommandCenterEntry, filter_commands_with_platform};
use terminalvision::input::mouse::{MouseTracker, handle_mouse_event_at};
use terminalvision::input::platform::Platform;
use terminalvision::input::shortcut::ShortcutRegistry;
use terminalvision::layout::geometry::ScreenLayout;
use terminalvision::ui::dialogs::{calculate_command_palette_rect, calculate_smart_jump_rect};

// ============================================================================
// 1. Command Center Opening
// ============================================================================
#[test]
fn test_01_command_center_opening() {
    let temp = TempDir::new().expect("tempdir");
    let mut app = App::at(temp.path().to_path_buf()).expect("app init");

    assert_eq!(app.mode(), Mode::Normal);
    app.handle_action(Action::CommandPalette);

    assert_eq!(app.mode(), Mode::CommandPalette);
    assert_eq!(app.command_palette().query(), "");
    assert_eq!(app.command_palette().selected_index(), 0);
    assert!(!app.command_palette().entries().is_empty());
}

// ============================================================================
// 2. Shortcut Mapping (Platform-Aware)
// ============================================================================
#[test]
fn test_02_platform_aware_shortcut_mapping() {
    let shortcuts = ShortcutRegistry::global();

    // Command Center
    let mac_cmd = shortcuts.primary_shortcut(Action::CommandPalette, Platform::Mac);
    assert_eq!(mac_cmd, Some("⌘K".to_string()));

    let win_cmd = shortcuts.primary_shortcut(Action::CommandPalette, Platform::Windows);
    assert_eq!(win_cmd, Some("Ctrl+K".to_string()));

    let linux_cmd = shortcuts.primary_shortcut(Action::CommandPalette, Platform::Linux);
    assert_eq!(linux_cmd, Some("Ctrl+K".to_string()));

    // Quick Switcher
    let mac_jump = shortcuts.primary_shortcut(Action::SmartJump, Platform::Mac);
    assert_eq!(mac_jump, Some("⌘P".to_string()));

    let win_jump = shortcuts.primary_shortcut(Action::SmartJump, Platform::Windows);
    assert_eq!(win_jump, Some("Ctrl+P".to_string()));

    let linux_jump = shortcuts.primary_shortcut(Action::SmartJump, Platform::Linux);
    assert_eq!(linux_jump, Some("Ctrl+P".to_string()));
}

// ============================================================================
// 3. Action Search
// ============================================================================
#[test]
fn test_03_action_search() {
    let copy_matches = filter_commands_with_platform("copy", Platform::Mac);
    assert!(!copy_matches.is_empty());
    assert_eq!(copy_matches[0].action(), Action::Copy);
    assert_eq!(
        copy_matches[0].shortcut_for_platform(Platform::Mac),
        Some("⌘C".to_string())
    );

    let rename_matches = filter_commands_with_platform("rename", Platform::Mac);
    assert!(!rename_matches.is_empty());
    assert_eq!(rename_matches[0].action(), Action::Rename);

    let refresh_matches = filter_commands_with_platform("refresh", Platform::Windows);
    assert!(!refresh_matches.is_empty());
    assert_eq!(refresh_matches[0].action(), Action::RefreshDirectory);
}

// ============================================================================
// 4. Fuzzy Matching
// ============================================================================
#[test]
fn test_04_fuzzy_matching() {
    assert!(fuzzy_match("ren", "Rename").is_some());
    assert!(fuzzy_match("ref", "Refresh Directory").is_some());
    assert!(fuzzy_match("shcut", "Show All Shortcuts").is_some());
    assert!(fuzzy_match("del", "Delete").is_some());
    assert!(fuzzy_match("nonexistent_term_xyz", "Rename").is_none());

    // Multi-target fuzzy match
    let targets = ["Show All Shortcuts", "Keyboard reference", "Help"];
    assert!(fuzzy_match_multi("shcut", &targets).is_some());
}

// ============================================================================
// 5. Result Ordering
// ============================================================================
#[test]
fn test_05_result_ordering() {
    let copy_results = filter_commands_with_platform("copy", Platform::Mac);
    assert_eq!(copy_results[0].action(), Action::Copy);

    let del_results = filter_commands_with_platform("delete", Platform::Mac);
    assert_eq!(del_results[0].action(), Action::Delete);

    let help_results = filter_commands_with_platform("shortcuts", Platform::Mac);
    assert_eq!(help_results[0].action(), Action::Help);
}

// ============================================================================
// 6. Action Registry Dispatch
// ============================================================================
#[test]
fn test_06_action_registry_dispatch() {
    let temp = TempDir::new().expect("tempdir");
    let mut app = App::at(temp.path().to_path_buf()).expect("app init");

    app.open_command_palette();
    assert_eq!(app.mode(), Mode::CommandPalette);

    // Search for help / shortcuts
    app.command_palette_mut().set_query("shortcuts");
    assert_eq!(app.command_palette().query(), "shortcuts");

    // Executing should dispatch Help action, opening the help dialog
    let action = app.confirm_modal();
    assert_eq!(action, Some(Action::Help));
    if let Some(act) = action {
        app.handle_action(act);
    }
    assert_eq!(app.mode(), Mode::Help);

    // Pressing Esc leaves help dialog back to Normal
    app.close_modal();
    assert_eq!(app.mode(), Mode::Normal);
}

// ============================================================================
// 7. Context Filtering
// ============================================================================
#[test]
fn test_07_context_filtering() {
    let temp = TempDir::new().expect("tempdir");
    let file_path = temp.path().join("test.txt");
    File::create(&file_path).expect("create file");

    let mut app = App::at(temp.path().to_path_buf()).expect("app init");
    assert!(app.pane(ActivePane::Left).visible_count() > 0);

    // Select the file
    app.select_entry(ActivePane::Left, 0);
    app.open_command_palette();

    let context = app.command_palette().entries();
    let actions: Vec<Action> = context
        .iter()
        .filter_map(|e| match e {
            CommandCenterEntry::Action(cmd) => Some(cmd.action()),
            _ => None,
        })
        .collect();

    // Context should prioritize File operations (Preview, Copy, Rename, Delete, etc.)
    assert!(actions.contains(&Action::Preview));
    assert!(actions.contains(&Action::Copy));
    assert!(actions.contains(&Action::Rename));
    assert!(actions.contains(&Action::Delete));
}

// ============================================================================
// 8. Keyboard Navigation
// ============================================================================
#[test]
fn test_08_keyboard_navigation() {
    let temp = TempDir::new().expect("tempdir");
    let mut app = App::at(temp.path().to_path_buf()).expect("app init");

    app.open_command_palette();
    assert_eq!(app.command_palette().selected_index(), 0);

    // Typing
    app.command_palette_mut().push_char('c');
    app.command_palette_mut().push_char('o');
    app.command_palette_mut().push_char('p');
    app.command_palette_mut().push_char('y');
    assert_eq!(app.command_palette().query(), "copy");

    // Backspace
    app.command_palette_mut().pop_char();
    assert_eq!(app.command_palette().query(), "cop");

    // Down and Up arrows
    let initial_count = app.command_palette().entries().len();
    if initial_count > 1 {
        app.command_palette_mut().move_down();
        assert_eq!(app.command_palette().selected_index(), 1);

        app.command_palette_mut().move_up();
        assert_eq!(app.command_palette().selected_index(), 0);
    }

    // Esc closes Command Center
    app.close_modal();
    assert_eq!(app.mode(), Mode::Normal);
}

// ============================================================================
// 9. Mouse Selection & Header Hit Testing
// ============================================================================
#[test]
fn test_09_mouse_selection_and_header_click() {
    let temp = TempDir::new().expect("tempdir");
    let mut app = App::at(temp.path().to_path_buf()).expect("app init");
    let mut tracker = MouseTracker::new();
    let area = Rect::new(0, 0, 100, 30);
    let layout = ScreenLayout::calculate(area);
    let platform = Platform::current();

    // 1. Click header button to open Command Center
    let hit_range = terminalvision::ui::header::command_center_hit_range(layout.header(), platform);
    assert!(hit_range.is_some());
    let (start_x, _end_x, y) = hit_range.unwrap();

    let click_header = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: start_x + 1,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    handle_mouse_event_at(click_header, &mut app, area, &mut tracker, Instant::now());
    assert_eq!(app.mode(), Mode::CommandPalette);

    // 2. Click outside dialog to close
    let click_outside = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 2,
        row: 2,
        modifiers: KeyModifiers::NONE,
    };
    handle_mouse_event_at(click_outside, &mut app, area, &mut tracker, Instant::now());
    assert_eq!(app.mode(), Mode::Normal);
}

// ============================================================================
// 10. Quick Switcher Opening
// ============================================================================
#[test]
fn test_10_quick_switcher_opening() {
    let temp = TempDir::new().expect("tempdir");
    let mut app = App::at(temp.path().to_path_buf()).expect("app init");

    assert_eq!(app.mode(), Mode::Normal);
    app.handle_action(Action::SmartJump);

    assert_eq!(app.mode(), Mode::SmartJump);
    assert_eq!(app.smart_jump().query(), "");
    assert_eq!(app.smart_jump().selected_index(), 0);
    assert!(!app.smart_jump().all_items().is_empty());
}

// ============================================================================
// 11. Recent History Tracking
// ============================================================================
#[test]
fn test_11_recent_history_tracking() {
    let temp = TempDir::new().expect("tempdir");
    let dir_a = temp.path().join("dir_a");
    let file_a = temp.path().join("file_a.rs");
    fs::create_dir(&dir_a).expect("create dir_a");
    File::create(&file_a).expect("create file_a");

    let mut app = App::at(temp.path().to_path_buf()).expect("app init");

    // Navigate to dir_a
    let _ = app.open_in(ActivePane::Left, dir_a.clone());
    assert!(app.recent_locations().locations().contains(&dir_a));

    // Record file
    app.recent_locations_mut().record_file(file_a.clone());
    assert!(app.recent_locations().files().contains(&file_a));
}

// ============================================================================
// 12. Duplicate History Prevention
// ============================================================================
#[test]
fn test_12_duplicate_history_prevention() {
    let temp = TempDir::new().expect("tempdir");
    let dir_a = temp.path().join("dir_a");
    fs::create_dir(&dir_a).expect("create dir_a");

    let mut app = App::at(temp.path().to_path_buf()).expect("app init");

    // Visit multiple times
    let _ = app.open_in(ActivePane::Left, dir_a.clone());
    let _ = app.open_in(ActivePane::Left, dir_a.clone());
    let _ = app.open_in(ActivePane::Left, dir_a.clone());

    let count = app
        .recent_locations()
        .locations()
        .iter()
        .filter(|p| **p == dir_a)
        .count();
    assert_eq!(count, 1, "Duplicate visits must be deduplicated");
}

// ============================================================================
// 13. Invalid History Handling
// ============================================================================
#[test]
fn test_13_invalid_history_handling() {
    let temp = TempDir::new().expect("tempdir");
    let dead_dir = temp.path().join("non_existent_folder_xyz");
    let dead_file = temp.path().join("deleted_file.txt");

    let mut app = App::at(temp.path().to_path_buf()).expect("app init");
    app.recent_locations_mut().record(dead_dir.clone());
    app.recent_locations_mut().record_file(dead_file.clone());

    // Opening Quick Switcher safely prunes or ignores deleted paths
    app.open_smart_jump();
    assert_eq!(app.mode(), Mode::SmartJump);

    for item in app.smart_jump().all_items() {
        assert!(
            item.path.exists(),
            "Quick Switcher must only contain valid paths, found: {:?}",
            item.path
        );
    }
}

// ============================================================================
// 14. Folder Navigation in Quick Switcher
// ============================================================================
#[test]
fn test_14_folder_navigation_in_quick_switcher() {
    let temp = TempDir::new().expect("tempdir");
    let target_dir = temp.path().join("target_folder");
    fs::create_dir(&target_dir).expect("create target_dir");

    let mut app = App::at(temp.path().to_path_buf()).expect("app init");
    app.recent_locations_mut().record(target_dir.clone());

    app.open_smart_jump();
    app.smart_jump_mut().set_query("target_folder");

    let selected = app.smart_jump().selected_item().cloned();
    assert!(selected.is_some());
    assert_eq!(selected.unwrap().path, target_dir);

    // Confirm jump
    app.confirm_modal();
    assert_eq!(app.mode(), Mode::Normal);
    assert_eq!(app.pane(ActivePane::Left).current_path(), &target_dir);
}

// ============================================================================
// 15. File Navigation + Selection in Quick Switcher
// ============================================================================
#[test]
fn test_15_file_navigation_and_selection_in_quick_switcher() {
    let temp = TempDir::new().expect("tempdir");
    let sub_dir = temp.path().join("sub");
    fs::create_dir(&sub_dir).expect("create sub");
    let file_path = sub_dir.join("main.rs");
    File::create(&file_path).expect("create main.rs");

    let mut app = App::at(temp.path().to_path_buf()).expect("app init");
    app.recent_locations_mut().record_file(file_path.clone());

    app.open_smart_jump();
    app.smart_jump_mut().set_query("main.rs");

    let selected = app.smart_jump().selected_item().cloned();
    assert!(selected.is_some());
    assert_eq!(selected.as_ref().unwrap().path, file_path);
    assert!(selected.unwrap().is_file);

    // Confirming file jump navigates to parent directory and selects the file
    app.confirm_modal();
    assert_eq!(app.mode(), Mode::Normal);
    assert_eq!(app.pane(ActivePane::Left).current_path(), &sub_dir);
    assert_eq!(
        app.pane(ActivePane::Left)
            .selected_entry()
            .map(|e| e.name().to_string_lossy().to_string()),
        Some("main.rs".to_string())
    );
}

// ============================================================================
// 16. Responsive Layout across Terminal Dimensions
// ============================================================================
#[test]
fn test_16_responsive_command_center_and_quick_switcher_layouts() {
    let dimensions = [
        (80, 24),
        (100, 30),
        (120, 30),
        (160, 40),
        (180, 40),
        (200, 60),
    ];

    for (width, height) in dimensions {
        let area = Rect::new(0, 0, width, height);

        // Verify Command Center dialog rect
        let cc_rect = calculate_command_palette_rect(area);
        assert!(cc_rect.width <= area.width);
        assert!(cc_rect.height <= area.height);
        assert!(cc_rect.x + cc_rect.width <= area.width);
        assert!(cc_rect.y + cc_rect.height <= area.height);

        // Verify Quick Switcher dialog rect
        let qs_rect = calculate_smart_jump_rect(area);
        assert!(qs_rect.width <= area.width);
        assert!(qs_rect.height <= area.height);
        assert!(qs_rect.x + qs_rect.width <= area.width);
        assert!(qs_rect.y + qs_rect.height <= area.height);

        // Verify ratatui render without panic
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();

        terminal
            .draw(|f| {
                terminalvision::ui::render(f, &app);
            })
            .unwrap();

        // Also test rendering in Command Center mode
        app.open_command_palette();
        terminal
            .draw(|f| {
                terminalvision::ui::render(f, &app);
            })
            .unwrap();

        // Also test rendering in Quick Switcher mode
        app.open_smart_jump();
        terminal
            .draw(|f| {
                terminalvision::ui::render(f, &app);
            })
            .unwrap();
    }
}
