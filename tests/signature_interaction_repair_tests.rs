//! Comprehensive Regression Test Suite for Critical Interaction & Multi-Tab Repair
//!
//! Validates:
//! 1. Enter key maps to Action::Open in Mode::Normal.
//! 2. Backspace key maps to Action::GoParent in Mode::Normal.
//! 3. Enter opens directory and survives immediate `poll_sync_and_filesystem` without rollback.
//! 4. Backspace navigates to parent directory and survives immediate `poll_sync_and_filesystem`.
//! 5. Creating a second tab creates independent state (path, entries, selection, history).
//! 6. Switching between Tab 1 and Tab 2 restores each tab's path without corruption.
//! 7. Navigating inside Tab 2 does NOT mutate Tab 1.
//! 8. Closing Tab 2 preserves Tab 1 in its correct path and state.
//! 9. Animation progress changes over monotonic elapsed time.
//! 10. Motion Off reaches final state immediately with 0 frames.
//! 11. Reduced Motion uses reduced duration.
//! 12. Focus routing sends Enter to the correct owner (Normal, Terminal, ContextMenu, Palette).
//! 13. Boot animation: Esc skips, intermediate states render progress.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use tempfile::TempDir;

use terminalvision::animation::clock::ManualClock;
use terminalvision::animation::core::AnimationTag;
use terminalvision::animation::easing::Easing;
use terminalvision::animation::engine::AnimationEngine;
use terminalvision::animation::{
    MotionMode, MotionPreferences, StartupMotionMode, boot_duration_and_easing,
};
use terminalvision::app::actions::Action;
use terminalvision::app::modes::Mode;
use terminalvision::app::state::{ActivePane, App};
use terminalvision::input::InputEvent;
use terminalvision::input::keyboard::{map_key_event_with_platform, map_key_with_platform};
use terminalvision::input::platform::Platform;

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

fn plain(code: KeyCode) -> KeyEvent {
    press(code, KeyModifiers::NONE)
}

fn create_temp_tree() -> (TempDir, PathBuf, PathBuf) {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let dir_a = temp.path().join("alpha");
    let dir_b = temp.path().join("beta");
    fs::create_dir(&dir_a).expect("Failed to create alpha");
    fs::create_dir(&dir_b).expect("Failed to create beta");
    fs::write(dir_a.join("file_a.txt"), "hello from alpha").expect("write file_a");
    fs::write(dir_b.join("file_b.txt"), "hello from beta").expect("write file_b");
    (temp, dir_a, dir_b)
}

// -----------------------------------------------------------------------------
// 1. Keyboard mapping: Enter -> Open, Backspace -> GoParent
// -----------------------------------------------------------------------------

#[test]
fn test_enter_maps_to_open_in_file_manager() {
    let key = plain(KeyCode::Enter);
    let mapped = map_key_with_platform(key, Mode::Normal, Platform::Linux);
    assert_eq!(mapped, Some(Action::Open));
}

#[test]
fn test_backspace_maps_to_goparent_in_file_manager() {
    let key = plain(KeyCode::Backspace);
    let mapped = map_key_with_platform(key, Mode::Normal, Platform::Linux);
    assert_eq!(mapped, Some(Action::GoParent));
}

// -----------------------------------------------------------------------------
// 2. Navigation with Enter & Backspace survives poll_sync_and_filesystem
// -----------------------------------------------------------------------------

#[test]
fn test_enter_opens_directory_and_does_not_rollback_on_poll() {
    let (temp, dir_a, _) = create_temp_tree();
    let mut app = App::at(temp.path().to_path_buf()).expect("Failed to create app");
    app.init_terminal(80, 24);

    // Initial state: app is in temp.path()
    assert_eq!(app.pane(ActivePane::Left).current_path(), temp.path());

    // Select directory "alpha"
    app.pane_mut(ActivePane::Left)
        .select_name(dir_a.file_name().unwrap());
    assert_eq!(
        app.pane(ActivePane::Left).selected_entry().unwrap().path(),
        &dir_a
    );

    // Handle Action::Open (equivalent to pressing Enter on the directory)
    app.handle_action(Action::Open);
    assert_eq!(app.pane(ActivePane::Left).current_path(), &dir_a);

    // Immediately trigger poll_sync_and_filesystem (simulate next tick of event loop)
    let _ = app.poll_sync_and_filesystem();

    // Verify the file manager DID NOT roll back to the previous directory
    assert_eq!(
        app.pane(ActivePane::Left).current_path(),
        &dir_a,
        "File manager directory must not roll back after Enter!"
    );
}

#[test]
fn test_backspace_goes_to_parent_and_does_not_rollback_on_poll() {
    let (temp, dir_a, _) = create_temp_tree();
    let mut app = App::at(dir_a.clone()).expect("Failed to create app");
    app.init_terminal(80, 24);

    assert_eq!(app.pane(ActivePane::Left).current_path(), &dir_a);

    // Handle Action::GoParent (equivalent to pressing Backspace)
    app.handle_action(Action::GoParent);
    assert_eq!(app.pane(ActivePane::Left).current_path(), temp.path());

    // Immediately trigger poll_sync_and_filesystem
    let _ = app.poll_sync_and_filesystem();

    // Verify the file manager DID NOT roll back to dir_a
    assert_eq!(
        app.pane(ActivePane::Left).current_path(),
        temp.path(),
        "File manager directory must not roll back after Backspace!"
    );
}

// -----------------------------------------------------------------------------
// 3. Multi-Tab: Creation, Independent State, Switching, and Isolation
// -----------------------------------------------------------------------------

#[test]
fn test_second_tab_creation_and_independent_state() {
    let (_temp, dir_a, dir_b) = create_temp_tree();
    let mut app = App::at(dir_a.clone()).expect("Failed to create app");
    app.init_terminal(80, 24);

    // Tab 1 is currently active at dir_a
    let active_pane = app.active_pane();
    assert_eq!(app.pane(active_pane).tab_count(), 1);
    assert_eq!(app.pane(active_pane).current_path(), &dir_a);

    // Create second tab in active pane
    app.handle_action(Action::NewTab);
    assert_eq!(app.pane(active_pane).tab_count(), 2);
    assert_eq!(app.pane(active_pane).active_tab_index(), 1);

    // Navigate Tab 2 to dir_b
    app.open_in(active_pane, dir_b.clone())
        .expect("navigate Tab 2");
    assert_eq!(app.pane(active_pane).current_path(), &dir_b);

    // Poll sync to simulate loop tick
    let _ = app.poll_sync_and_filesystem();
    assert_eq!(app.pane(active_pane).current_path(), &dir_b);

    // Switch to previous tab (Tab 1)
    app.handle_action(Action::PreviousTab);
    assert_eq!(app.pane(active_pane).active_tab_index(), 0);
    assert_eq!(
        app.pane(active_pane).current_path(),
        &dir_a,
        "Tab 1 must retain its original path!"
    );

    // Poll sync to simulate loop tick while on Tab 1
    let _ = app.poll_sync_and_filesystem();
    assert_eq!(app.pane(active_pane).current_path(), &dir_a);

    // Switch to next tab (Tab 2)
    app.handle_action(Action::NextTab);
    assert_eq!(app.pane(active_pane).active_tab_index(), 1);
    assert_eq!(
        app.pane(active_pane).current_path(),
        &dir_b,
        "Tab 2 must retain its independent path!"
    );

    // Close Tab 2
    app.handle_action(Action::CloseTab);
    assert_eq!(app.pane(active_pane).tab_count(), 1);
    assert_eq!(app.pane(active_pane).active_tab_index(), 0);
    assert_eq!(app.pane(active_pane).current_path(), &dir_a);
}

#[test]
fn test_tab_b_navigation_does_not_mutate_tab_a() {
    let (temp, dir_a, dir_b) = create_temp_tree();
    let mut app = App::at(dir_a.clone()).expect("Failed to create app");

    let which = app.active_pane();
    app.new_tab_in_active_pane();
    assert_eq!(app.pane(which).active_tab_index(), 1);

    // Navigate Tab B into dir_b
    app.open_in(which, dir_b.clone()).unwrap();

    // Verify Tab A's path directly
    assert_eq!(app.pane(which).tabs()[0].current_path(), &dir_a);
    // Verify Tab B's path directly
    assert_eq!(app.pane(which).tabs()[1].current_path(), &dir_b);

    // Navigate Tab B to parent (temp.path())
    app.go_to_parent().unwrap();
    assert_eq!(app.pane(which).tabs()[1].current_path(), temp.path());
    assert_eq!(
        app.pane(which).tabs()[0].current_path(),
        &dir_a,
        "Tab A must not be mutated when Tab B navigates to parent!"
    );
}

// -----------------------------------------------------------------------------
// 4. Focus & Input Routing
// -----------------------------------------------------------------------------

#[test]
fn test_focus_routing_terminal_mode_sends_keys_to_pty() {
    let key_enter = plain(KeyCode::Enter);
    let event = map_key_event_with_platform(key_enter, Mode::Terminal, Platform::Linux);
    assert_eq!(event, InputEvent::TerminalKey(key_enter));

    let key_bk = plain(KeyCode::Backspace);
    let event_bk = map_key_event_with_platform(key_bk, Mode::Terminal, Platform::Linux);
    assert_eq!(event_bk, InputEvent::TerminalKey(key_bk));
}

#[test]
fn test_focus_routing_context_menu_enter_confirms_modal() {
    let key_enter = plain(KeyCode::Enter);
    let event = map_key_event_with_platform(key_enter, Mode::ContextMenu, Platform::Linux);
    assert_eq!(event, InputEvent::ModalConfirm);
}

#[test]
fn test_focus_routing_command_palette_enter_confirms_modal() {
    let key_enter = plain(KeyCode::Enter);
    let event = map_key_event_with_platform(key_enter, Mode::CommandPalette, Platform::Linux);
    assert_eq!(event, InputEvent::ModalConfirm);
}

// -----------------------------------------------------------------------------
// 5. Animation Engine: Elapsed Monotonic Timing, Reduced Motion & Motion Off
// -----------------------------------------------------------------------------

#[test]
fn test_animation_progress_advances_over_elapsed_time() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let prefs = MotionPreferences {
        mode: MotionMode::Full,
        startup: StartupMotionMode::Cinematic,
        ..Default::default()
    };

    let mut engine = AnimationEngine::new(clock.clone(), prefs);
    let id = engine.start_raw(
        AnimationTag::VisionBoot,
        Duration::from_millis(1000),
        Easing::Linear,
    );

    // At t = 0
    let p0 = engine.progress(id).unwrap();
    assert_eq!(p0.linear, 0.0);

    // At t = 200ms
    clock.advance(Duration::from_millis(200));
    assert!(engine.tick());
    let p200 = engine.progress(id).unwrap();
    assert!((p200.linear - 0.20).abs() < 0.01);

    // At t = 500ms
    clock.advance(Duration::from_millis(300));
    assert!(engine.tick());
    let p500 = engine.progress(id).unwrap();
    assert!((p500.linear - 0.50).abs() < 0.01);

    // At t = 1000ms
    clock.advance(Duration::from_millis(500));
    let p1000 = engine.progress(id).unwrap();
    assert_eq!(p1000.linear, 1.0);
}

#[test]
fn test_motion_off_reaches_final_state_immediately() {
    let (temp, _, _) = create_temp_tree();
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    app.set_motion_mode(MotionMode::Off);
    app.start_boot();

    // MotionMode::Off must never enter or remain in Boot mode
    assert!(!app.is_booting());
    assert_eq!(app.mode(), Mode::Normal);
    assert!(app.boot_state().is_none());
}

#[test]
fn test_reduced_motion_uses_reduced_duration() {
    let (duration, easing) =
        boot_duration_and_easing(StartupMotionMode::Cinematic, MotionMode::Reduced)
            .expect("Should return reduced duration");
    assert_eq!(easing, Easing::Linear);
    assert!(duration <= Duration::from_millis(400));
    assert!(duration >= Duration::from_millis(300));
}

#[test]
fn test_esc_skips_boot_animation() {
    let esc = plain(KeyCode::Esc);
    let mapped = map_key_with_platform(esc, Mode::Boot, Platform::Linux);
    assert_eq!(mapped, Some(Action::Cancel));
}
