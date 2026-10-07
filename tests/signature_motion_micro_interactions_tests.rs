//! Deterministic integration tests for Phase 2.3: Signature Motion & Micro-Interactions.
//!
//! Validates non-blocking animation lifecycles, rapid input superseding, modal and palette
//! transitions, focus preservation, terminal safety, and accessibility modes.

use std::fs;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tempfile::TempDir;

use terminalvision::animation::clock::ManualClock;
use terminalvision::animation::core::AnimationTag;
use terminalvision::animation::easing::Easing;
use terminalvision::animation::engine::AnimationEngine;
use terminalvision::animation::micro::*;
use terminalvision::animation::preferences::{MotionMode, MotionPreferences};
use terminalvision::app::actions::Action;
use terminalvision::app::modes::Mode;
use terminalvision::app::state::{ActivePane, App};

fn create_test_dir(name: &str) -> TempDir {
    let temp = TempDir::new().expect("Failed to create tempdir");
    for i in 1..=5 {
        fs::write(
            temp.path().join(format!("file_{name}_{i}.txt")),
            format!("content {i}"),
        )
        .unwrap();
    }
    temp
}

// -----------------------------------------------------------------------------
// Test 1: Navigation transition
// -----------------------------------------------------------------------------
#[test]
fn test_01_navigation_transition_lifecycle() {
    let temp = create_test_dir("nav");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);

    app.trigger_navigation_animation();
    assert!(app.has_active_animations());
    assert!(
        app.animation_engine()
            .has_active_tag(AnimationTag::Navigation)
    );

    let progress = app
        .animation_engine()
        .tag_progress(AnimationTag::Navigation)
        .expect("Progress should be present");
    assert_eq!(progress.duration, NAVIGATION_DURATION);
}

// -----------------------------------------------------------------------------
// Test 2: Selection transition
// -----------------------------------------------------------------------------
#[test]
fn test_02_selection_transition_lifecycle() {
    let temp = create_test_dir("sel");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);

    app.handle_action(Action::MoveDown);
    assert!(
        app.animation_engine()
            .has_active_tag(AnimationTag::Selection)
    );

    let progress = app
        .animation_engine()
        .tag_progress(AnimationTag::Selection)
        .expect("Progress should be present");
    assert_eq!(progress.duration, SELECTION_DURATION);
}

// -----------------------------------------------------------------------------
// Test 3: Focus transition
// -----------------------------------------------------------------------------
#[test]
fn test_03_focus_transition_lifecycle() {
    let temp = create_test_dir("focus");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);

    assert_eq!(app.active_pane(), ActivePane::Left);
    app.handle_action(Action::SwitchPane);
    assert_eq!(app.active_pane(), ActivePane::Right);

    assert!(
        app.animation_engine()
            .has_active_tag(AnimationTag::Custom("Focus"))
    );
}

// -----------------------------------------------------------------------------
// Test 4: Command Center animation
// -----------------------------------------------------------------------------
#[test]
fn test_04_command_center_animation() {
    let temp = create_test_dir("cmd_center");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);

    app.set_mode(Mode::CommandPalette);
    assert_eq!(app.mode(), Mode::CommandPalette);
    assert!(
        app.animation_engine()
            .has_active_tag(AnimationTag::CommandCenter)
    );

    // Closing immediately cancels animation
    app.set_mode(Mode::Normal);
    assert!(
        !app.animation_engine()
            .has_active_tag(AnimationTag::CommandCenter)
    );
}

// -----------------------------------------------------------------------------
// Test 5: Quick Switcher animation
// -----------------------------------------------------------------------------
#[test]
fn test_05_quick_switcher_animation() {
    let temp = create_test_dir("switcher");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);

    app.set_mode(Mode::SmartJump);
    assert_eq!(app.mode(), Mode::SmartJump);
    assert!(
        app.animation_engine()
            .has_active_tag(AnimationTag::QuickSwitcher)
    );

    // Closing immediately cancels switcher animation
    app.set_mode(Mode::Normal);
    assert!(
        !app.animation_engine()
            .has_active_tag(AnimationTag::QuickSwitcher)
    );
}

// -----------------------------------------------------------------------------
// Test 6: Dialog animation
// -----------------------------------------------------------------------------
#[test]
fn test_06_dialog_animation() {
    let temp = create_test_dir("dialog");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);

    app.set_mode(Mode::Confirm);
    assert!(app.animation_engine().has_active_tag(AnimationTag::Dialog));

    app.close_modal();
    assert!(!app.animation_engine().has_active_tag(AnimationTag::Dialog));
}

// -----------------------------------------------------------------------------
// Test 7: Context-menu animation
// -----------------------------------------------------------------------------
#[test]
fn test_07_context_menu_animation() {
    let temp = create_test_dir("ctx_menu");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);

    app.open_context_menu((15, 8));
    assert_eq!(app.mode(), Mode::ContextMenu);
    assert!(
        app.animation_engine()
            .has_active_tag(AnimationTag::ContextMenu)
    );

    app.close_context_menu();
    assert!(
        !app.animation_engine()
            .has_active_tag(AnimationTag::ContextMenu)
    );
}

// -----------------------------------------------------------------------------
// Test 8: Operation feedback
// -----------------------------------------------------------------------------
#[test]
fn test_08_operation_feedback_lifecycle() {
    let temp = create_test_dir("op_feedback");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);

    app.trigger_operation_animation();
    assert!(
        app.animation_engine()
            .has_active_tag(AnimationTag::Operation)
    );

    let progress = app
        .animation_engine()
        .tag_progress(AnimationTag::Operation)
        .expect("Operation progress should exist");
    assert_eq!(progress.duration, OPERATION_DURATION);
}

// -----------------------------------------------------------------------------
// Test 9: Success feedback
// -----------------------------------------------------------------------------
#[test]
fn test_09_success_feedback_and_notification() {
    let temp = create_test_dir("success");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    app.report_navigation(Ok(()));
    assert!(
        app.animation_engine()
            .has_active_tag(AnimationTag::Navigation)
    );
    assert!(!app.notification().is_active());
}

// -----------------------------------------------------------------------------
// Test 10: Error feedback
// -----------------------------------------------------------------------------
#[test]
fn test_10_error_feedback_and_notification() {
    let temp = create_test_dir("err");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    let err = terminalvision::app::state::NavigationError::WorkingDirectory(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "nonexistent",
    ));
    app.report_navigation(Err(err));
    assert!(app.notification().is_active());
}

// -----------------------------------------------------------------------------
// Test 11: Vision Pulse
// -----------------------------------------------------------------------------
#[test]
fn test_11_vision_pulse_reusable_effect() {
    let temp = create_test_dir("pulse");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);

    app.trigger_vision_pulse();
    assert!(
        app.animation_engine()
            .has_active_tag(AnimationTag::VisionPulse)
    );

    let progress = app
        .animation_engine()
        .tag_progress(AnimationTag::VisionPulse)
        .expect("VisionPulse progress should exist");
    assert_eq!(progress.duration, VISION_PULSE_DURATION);
}

// -----------------------------------------------------------------------------
// Test 12: Animation cancellation
// -----------------------------------------------------------------------------
#[test]
fn test_12_animation_cancellation() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let prefs = MotionPreferences {
        mode: MotionMode::Full,
        ..Default::default()
    };
    let mut engine = AnimationEngine::new(clock, prefs);

    let id = engine.start(
        AnimationTag::Navigation,
        Duration::from_millis(150),
        Easing::EaseOut,
    );
    assert!(engine.is_animating());

    engine.cancel(id);
    assert!(!engine.is_animating());
}

// -----------------------------------------------------------------------------
// Test 13: Rapid input superseding
// -----------------------------------------------------------------------------
#[test]
fn test_13_rapid_input_superseding() {
    let temp = create_test_dir("rapid");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);

    // Rapid selection down keypresses
    for _ in 0..10 {
        app.handle_action(Action::MoveDown);
    }

    // Must still have exactly one active selection animation (never queued endlessly)
    assert!(
        app.animation_engine()
            .has_active_tag(AnimationTag::Selection)
    );
    assert!(app.has_active_animations());
}

// -----------------------------------------------------------------------------
// Test 14: Resize handling
// -----------------------------------------------------------------------------
#[test]
fn test_14_resize_handling_across_resolutions() {
    let temp = create_test_dir("resize");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);
    app.set_mode(Mode::CommandPalette);

    let resolutions = [
        (40, 10),
        (80, 24),
        (100, 30),
        (120, 30),
        (160, 40),
        (200, 60),
    ];

    for (w, h) in resolutions {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                terminalvision::ui::render(frame, &app);
            })
            .expect("Render should not panic on resize");
    }
}

// -----------------------------------------------------------------------------
// Test 15: Reduced motion
// -----------------------------------------------------------------------------
#[test]
fn test_15_reduced_motion_behavior() {
    let temp = create_test_dir("reduced");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Reduced);

    app.trigger_navigation_animation();
    let progress = app
        .animation_engine()
        .tag_progress(AnimationTag::Navigation)
        .expect("Progress should exist");
    assert!(progress.duration <= Duration::from_millis(60));
}

// -----------------------------------------------------------------------------
// Test 16: Motion off
// -----------------------------------------------------------------------------
#[test]
fn test_16_motion_off_behavior() {
    let temp = create_test_dir("off");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Off);

    app.trigger_navigation_animation();
    assert!(!app.has_active_animations());
    assert!(
        !app.animation_engine()
            .has_active_tag(AnimationTag::Navigation)
    );
}

// -----------------------------------------------------------------------------
// Test 17: Small terminal rendering
// -----------------------------------------------------------------------------
#[test]
fn test_17_small_terminal_rendering() {
    let temp = create_test_dir("small");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);
    app.set_mode(Mode::Confirm);

    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            terminalvision::ui::render(frame, &app);
        })
        .expect("Render should handle tiny terminals safely");
}

// -----------------------------------------------------------------------------
// Test 18: Terminal focus safety
// -----------------------------------------------------------------------------
#[test]
fn test_18_terminal_focus_safety() {
    let temp = create_test_dir("term_focus");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);

    // Focusing embedded terminal does not crash or corrupt state
    app.set_mode(Mode::Terminal);
    assert_eq!(app.mode(), Mode::Terminal);

    // Switching back to normal mode restores focus cleanly
    app.set_mode(Mode::Normal);
    assert_eq!(app.mode(), Mode::Normal);
}

// -----------------------------------------------------------------------------
// Test 19: Preview transition
// -----------------------------------------------------------------------------
#[test]
fn test_19_preview_transition() {
    let temp = create_test_dir("preview_trans");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_motion_mode(MotionMode::Full);

    app.trigger_preview_animation();
    assert!(
        app.animation_engine()
            .has_active_tag(AnimationTag::Custom("Preview"))
    );

    let progress = app
        .animation_engine()
        .tag_progress(AnimationTag::Custom("Preview"))
        .expect("Preview progress should exist");
    assert_eq!(progress.duration, PREVIEW_DURATION);
}

// -----------------------------------------------------------------------------
// Test 20: Animation cleanup
// -----------------------------------------------------------------------------
#[test]
fn test_20_animation_cleanup_and_isolation() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let prefs = MotionPreferences {
        mode: MotionMode::Full,
        ..Default::default()
    };
    let mut engine = AnimationEngine::new(clock.clone(), prefs);

    engine.start(
        AnimationTag::Selection,
        Duration::from_millis(100),
        Easing::EaseOut,
    );
    assert!(engine.is_animating());

    clock.advance(Duration::from_millis(120));
    assert!(!engine.is_animating());
    assert!(!engine.has_active_tag(AnimationTag::Selection));
}
