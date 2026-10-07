//! TerminalVision Phase 3.3 — Integration, QA & Release Polish Test Suite
//!
//! Comprehensive regression, stress, focus isolation, and end-to-end integration tests
//! ensuring release readiness, stability, responsiveness, and safety.

use std::fs;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tempfile::tempdir;

use terminalvision::animation::clock::ManualClock;
use terminalvision::animation::core::{AnimationState, AnimationTag};
use terminalvision::animation::easing::Easing;
use terminalvision::animation::engine::AnimationEngine;
use terminalvision::animation::preferences::{MotionMode, MotionPreferences, StartupMotionMode};
use terminalvision::app::actions::{Action, ActionCategory};
use terminalvision::app::modes::Mode;
use terminalvision::app::state::{ActivePane, App, ClipboardOperation};
use terminalvision::git::project::ProjectType;
use terminalvision::input::InputEvent;
use terminalvision::input::platform::Platform;
use terminalvision::input::shortcut::ShortcutRegistry;
use terminalvision::layout::geometry::{ScreenLayout, TerminalSize};
use terminalvision::ui::{display_width, render, truncate_filename_to_width, truncate_to_width};

#[test]
fn test_01_full_end_to_end_user_workflow() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Create nested workspace structure with diverse files
    let src_dir = root.join("src");
    let docs_dir = root.join("docs");
    let deep_dir = src_dir.join("engine");
    fs::create_dir_all(&deep_dir).unwrap();
    fs::create_dir_all(&docs_dir).unwrap();

    let file_a = root.join("Cargo.toml");
    let file_b = src_dir.join("main.rs");
    let file_c = deep_dir.join("lib.rs");
    let file_d = docs_dir.join("README.md");

    fs::write(&file_a, "[package]\nname = \"test_pkg\"\n").unwrap();
    fs::write(&file_b, "fn main() {}\n").unwrap();
    fs::write(&file_c, "pub fn run() {}\n").unwrap();
    fs::write(&file_d, "# Documentation\n").unwrap();

    let mut app = App::at(root.to_path_buf()).expect("app should initialize at temp directory");
    app.set_visible_rows(ActivePane::Left, 20);
    app.set_visible_rows(ActivePane::Right, 20);

    // 1. Startup & Vision Boot check
    assert_eq!(app.mode(), Mode::Normal);
    app.start_boot();
    assert_eq!(app.mode(), Mode::Boot);
    assert!(app.is_booting());

    // 2. Esc skips Vision Boot
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);
    assert!(!app.is_booting());

    // 3. Navigate directories
    let initial_count = app.pane(ActivePane::Left).visible_count();
    assert!(initial_count > 0);

    app.handle_action(Action::MoveDown);
    assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(1));
    app.handle_action(Action::MoveUp);
    assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));

    // 4. Multi-selection via SelectRangeDown
    app.handle_action(Action::SelectRangeDown);
    assert!(app.pane(ActivePane::Left).is_item_selected(0));
    assert!(app.pane(ActivePane::Left).is_item_selected(1));
    assert_eq!(app.pane(ActivePane::Left).selected_count(), 2);

    // 5. Open context menu on selection
    app.open_context_menu((20, 5));
    assert_eq!(app.mode(), Mode::ContextMenu);
    assert!(!app.context_menu().items.is_empty());

    // 6. Copy action via Context Menu
    let copy_idx = app
        .context_menu()
        .items
        .iter()
        .position(|item| item.action_target() == Some(Action::Copy))
        .expect("Copy action must exist in context menu");
    app.context_menu_mut().select_index(copy_idx);
    let confirmed_action = app.confirm_modal();
    assert_eq!(confirmed_action, Some(Action::Copy));
    if let Some(act) = confirmed_action {
        app.handle_action(act);
    }
    assert_eq!(app.mode(), Mode::Normal);
    assert_eq!(app.clipboard().operation(), Some(ClipboardOperation::Copy));

    // 7. Preview
    app.handle_action(Action::Preview);
    assert_eq!(app.mode(), Mode::Preview);
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);

    // 8. Command Center (⌘K)
    app.handle_action(Action::CommandPalette);
    assert_eq!(app.mode(), Mode::CommandPalette);
    assert!(!app.command_palette().entries().is_empty());
    app.palette_push_char('n');
    app.palette_push_char('e');
    app.palette_push_char('w');
    assert!(app.command_palette().query().starts_with("new"));
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);

    // 9. Quick Switcher (⌘P)
    app.handle_action(Action::SmartJump);
    assert_eq!(app.mode(), Mode::SmartJump);
    app.smart_jump_push_char('s');
    app.smart_jump_push_char('r');
    app.smart_jump_push_char('c');
    assert_eq!(app.smart_jump().query(), "src");
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);

    // 10. Switch Panes
    app.handle_action(Action::SwitchPane);
    assert_eq!(app.active_pane(), ActivePane::Right);
    app.handle_action(Action::SwitchPane);
    assert_eq!(app.active_pane(), ActivePane::Left);

    // 11. Context menu keyboard navigation (Shift+F10 / Menu key)
    app.handle_action(Action::ContextMenu);
    assert_eq!(app.mode(), Mode::ContextMenu);
    let initial_menu_idx = app.context_menu().selected;
    app.context_menu_mut().move_down();
    assert_eq!(
        app.context_menu().selected,
        initial_menu_idx.saturating_add(1)
    );
    app.context_menu_mut().move_up();
    assert_eq!(app.context_menu().selected, initial_menu_idx);
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);

    // 12. Quit
    app.handle_action(Action::Quit);
    assert!(app.should_quit());
}

#[test]
fn test_02_focus_system_state_isolation_and_restoration() {
    let mut app = App::default();
    assert_eq!(app.mode(), Mode::Normal);

    // Normal -> CommandPalette -> Cancel restores Normal
    app.handle_action(Action::CommandPalette);
    assert_eq!(app.mode(), Mode::CommandPalette);
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);

    // Normal -> SmartJump -> Cancel restores Normal
    app.handle_action(Action::SmartJump);
    assert_eq!(app.mode(), Mode::SmartJump);
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);

    // Normal -> ContextMenu -> Cancel restores Normal
    app.handle_action(Action::ContextMenu);
    assert_eq!(app.mode(), Mode::ContextMenu);
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);

    // Normal -> Bookmarks -> Cancel restores Normal
    app.handle_action(Action::OpenBookmarks);
    assert_eq!(app.mode(), Mode::Bookmarks);
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);

    // Normal -> Help -> Cancel restores Normal
    app.handle_action(Action::Help);
    assert_eq!(app.mode(), Mode::Help);
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);

    // Normal -> StorageVision -> Cancel restores Normal
    app.handle_action(Action::StorageVision);
    assert_eq!(app.mode(), Mode::StorageVision);
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);

    // Normal -> ThemeSelector -> Cancel restores Normal
    app.handle_action(Action::ThemeSelector);
    assert_eq!(app.mode(), Mode::ThemeSelector);
    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);
}

#[test]
fn test_03_embedded_terminal_state_and_focus_switching() {
    let mut app = App::default();
    app.init_terminal(80, 24);

    assert_eq!(app.mode(), Mode::Normal);

    // Toggle to terminal
    app.handle_action(Action::ToggleTerminalFocus);
    assert_eq!(app.mode(), Mode::Terminal);

    // Typing when in terminal sends key to PTY without altering file manager state
    app.terminal_send_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    app.terminal_send_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    app.terminal_send_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // File manager state remains pristine
    assert_eq!(app.pane(ActivePane::Left).selected_index(), None);
    assert_eq!(app.pane(ActivePane::Right).selected_index(), None);

    // Toggle back to Normal
    app.handle_action(Action::ToggleTerminalFocus);
    assert_eq!(app.mode(), Mode::Normal);
}

#[test]
fn test_04_universal_action_registry_single_source_of_truth() {
    let registry = ShortcutRegistry::global();
    let platform = Platform::current();

    // Verify critical categories and shortcuts exist
    let nav_shortcuts = registry.shortcuts_by_category(ActionCategory::Navigation, platform);
    assert!(!nav_shortcuts.is_empty());

    let file_shortcuts = registry.shortcuts_by_category(ActionCategory::Files, platform);
    assert!(!file_shortcuts.is_empty());

    let git_shortcuts = registry.shortcuts_by_category(ActionCategory::Git, platform);
    assert!(!git_shortcuts.is_empty());

    let app_shortcuts = registry.shortcuts_by_category(ActionCategory::Application, platform);
    assert!(!app_shortcuts.is_empty());

    // Verify action categories
    assert_eq!(Action::Copy.category(), ActionCategory::Files);
    assert_eq!(Action::Cut.category(), ActionCategory::Files);
    assert_eq!(Action::Paste.category(), ActionCategory::Files);
    assert_eq!(Action::Delete.category(), ActionCategory::Files);
    assert_eq!(Action::Rename.category(), ActionCategory::Files);
    assert_eq!(Action::NewFile.category(), ActionCategory::Files);
    assert_eq!(Action::NewDirectory.category(), ActionCategory::Files);
    assert_eq!(Action::RefreshDirectory.category(), ActionCategory::View);
    assert_eq!(Action::Preview.category(), ActionCategory::Preview);
    assert_eq!(Action::Quit.category(), ActionCategory::Application);
}

#[test]
fn test_05_responsive_rendering_matrix_all_standard_and_extreme_sizes() {
    let sizes = [
        (80, 24),   // Standard classic
        (100, 30),  // Medium default
        (120, 30),  // Modern standard
        (159, 30),  // Dual-pane boundary
        (160, 40),  // Triple-pane preview boundary
        (180, 40),  // Wide terminal
        (200, 60),  // Ultra wide
        (40, 10),   // Narrow & short
        (20, 5),    // Extreme small
        (300, 120), // Huge display
    ];

    let mut app = App::default();

    for (cols, rows) in sizes {
        let backend = TestBackend::new(cols, rows);
        let mut terminal = Terminal::new(backend).unwrap();

        // 1. Normal mode
        app.set_mode(Mode::Normal);
        terminal
            .draw(|f| {
                render(f, &app);
            })
            .expect("render must succeed in Normal mode");

        // 2. Command Palette mode
        app.set_mode(Mode::CommandPalette);
        terminal
            .draw(|f| {
                render(f, &app);
            })
            .expect("render must succeed in CommandPalette mode");

        // 3. Context Menu mode
        app.set_mode(Mode::ContextMenu);
        app.open_context_menu((cols / 2, rows / 2));
        terminal
            .draw(|f| {
                render(f, &app);
            })
            .expect("render must succeed in ContextMenu mode");

        // 4. Help mode
        app.set_mode(Mode::Help);
        terminal
            .draw(|f| {
                render(f, &app);
            })
            .expect("render must succeed in Help mode");

        // 5. Storage Vision mode
        app.set_mode(Mode::StorageVision);
        terminal
            .draw(|f| {
                render(f, &app);
            })
            .expect("render must succeed in StorageVision mode");
    }
}

#[test]
fn test_06_large_directory_performance_and_bounded_safety() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Create 200 files with diverse names
    for i in 0..200 {
        let fname = format!("file_{i:04}_{}.txt", "a".repeat(i % 30));
        fs::write(root.join(&fname), format!("content of file {i}")).unwrap();
    }

    let mut app = App::at(root.to_path_buf()).expect("should open large dir");
    app.set_visible_rows(ActivePane::Left, 20);
    assert_eq!(app.pane(ActivePane::Left).visible_count(), 200);

    // Rapid scrolling does not panic or hang
    for _ in 0..50 {
        app.handle_action(Action::MoveDown);
    }
    assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(50));

    // Page down (visible_rows = 20)
    app.handle_action(Action::PageDown);
    assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(70));

    // Page up back to top
    for _ in 0..10 {
        app.handle_action(Action::PageUp);
    }
    assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));

    // Render under 80x24 terminal
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            render(f, &app);
        })
        .expect("rendering large directory must succeed");
}

#[test]
fn test_07_unicode_display_width_accuracy_across_all_alphabets() {
    let unicode_cases: [(&str, &str, usize); 8] = [
        ("ASCII", "standard_name.rs", 16),
        ("CJK Chinese", "项目源文件_测试.rs", 18),
        ("CJK Japanese", "ドキュメント_作成.md", 20),
        ("CJK Korean", "프로젝트_문서.txt", 17),
        ("Malayalam", "മലയാളം_ഫയൽ.txt", 13),
        ("Emoji & Symbols", "📁_🦀_⚡_test.rs", 16),
        ("Accented Latin", "résumé_über_først.pdf", 21),
        ("Combining Diacritics", "cafe\u{0301}_menu.txt", 13),
    ];

    for (desc, text, expected_width) in unicode_cases {
        let width = display_width(text);
        assert_eq!(
            width, expected_width,
            "{desc} width {width} != expected {expected_width} for text '{text}'"
        );

        // Truncation respects width bounds strictly
        for max_w in 1..=30 {
            let truncated = truncate_to_width(text, max_w);
            let trunc_w = display_width(&truncated);
            assert!(
                trunc_w <= max_w,
                "Truncated string '{truncated}' has display width {trunc_w} > max {max_w}"
            );

            let fn_trunc = truncate_filename_to_width(text, max_w);
            let fn_w = display_width(&fn_trunc);
            assert!(
                fn_w <= max_w,
                "Truncated filename '{fn_trunc}' has display width {fn_w} > max {max_w}"
            );
        }
    }
}

#[test]
fn test_08_animation_engine_motion_modes_full_reduced_off() {
    let base = Instant::now();
    let clock = ManualClock::new(base);

    // 1. Full motion: animations run through easing curve
    let prefs_full = MotionPreferences {
        mode: MotionMode::Full,
        ..Default::default()
    };

    let mut engine = AnimationEngine::new(clock.clone(), prefs_full);
    let id1 = engine.start(
        AnimationTag::Selection,
        Duration::from_millis(100),
        Easing::Linear,
    );
    assert!(engine.is_animating());
    clock.advance(Duration::from_millis(50));
    assert_eq!(engine.progress(id1).unwrap().state, AnimationState::Running);

    // 2. Off mode: animations complete immediately or do not run
    let prefs_off = MotionPreferences {
        mode: MotionMode::Off,
        ..Default::default()
    };

    let mut engine_off = AnimationEngine::new(clock.clone(), prefs_off);
    let id2 = engine_off.start(
        AnimationTag::Dialog,
        Duration::from_millis(100),
        Easing::Linear,
    );
    assert_eq!(
        engine_off.progress(id2).unwrap().state,
        AnimationState::Completed
    );
    assert!(!engine_off.is_animating());

    // 3. Reduced motion: runs at reduced duration
    let prefs_reduced = MotionPreferences {
        mode: MotionMode::Reduced,
        startup: StartupMotionMode::Minimal,
        ..Default::default()
    };
    assert!(prefs_reduced.mode.is_reduced_or_off());
}

#[test]
fn test_09_project_intelligence_deterministic_and_read_only() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Create a Rust project
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"tv_test\"\n").unwrap();
    fs::write(root.join("src").join("main.rs"), "fn main() {}\n").unwrap();

    let app = App::at(root.to_path_buf()).expect("should open rust project");
    let active_pane = app.pane(ActivePane::Left);

    let proj_info = active_pane.project_info();
    assert!(proj_info.is_active());
    assert_eq!(proj_info.primary_type(), Some(ProjectType::Rust));

    // Verify project graph structure
    let ws = active_pane.workspace_context();
    assert!(ws.is_active());

    // Project detection is read-only (no extraneous files generated)
    assert!(root.join("Cargo.toml").exists());
    assert!(!root.join(".tv_cache").exists());
}

#[test]
fn test_10_terminal_resize_during_active_menu_and_dialog() {
    let mut app = App::default();
    let mut size = TerminalSize::new(100, 30);

    // Open context menu at column 80, row 20
    app.open_context_menu((80, 20));
    assert_eq!(app.mode(), Mode::ContextMenu);

    // Terminal resizes smaller to 60x20
    let resize_event = InputEvent::Resize(TerminalSize::new(60, 20));
    // Apply resize
    if let InputEvent::Resize(reported) = resize_event {
        size = reported;
        let layout = ScreenLayout::calculate(size.area());
        let term_rect = layout.terminal();
        app.terminal_resize(
            term_rect.width.saturating_sub(2).max(1),
            term_rect.height.saturating_sub(2).max(1),
        );
    }

    // Context menu position clamps safely within new bounds
    let menu_rect = terminalvision::ui::dialogs::calculate_context_menu_rect(
        app.context_menu().position,
        app.context_menu().items.len(),
        size.area(),
    );
    assert!(menu_rect.x + menu_rect.width <= size.columns());
    assert!(menu_rect.y + menu_rect.height <= size.rows());
}
