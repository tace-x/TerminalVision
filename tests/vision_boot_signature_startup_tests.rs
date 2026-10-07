//! Vision Boot Signature Startup Experience Verification Tests (Phase 2.2).
//!
//! 20 Comprehensive tests validating state machine transitions, timing, cancellation,
//! real initialization reporting, project awareness, Git awareness, small screen safety,
//! Unicode/ASCII fallbacks, and focus restoration without flakiness or real-time sleep delays.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tempfile::TempDir;

use terminalvision::animation::clock::ManualClock;
use terminalvision::animation::core::AnimationTag;
use terminalvision::animation::easing::Easing;
use terminalvision::animation::engine::AnimationEngine;
use terminalvision::animation::{
    BootPhase, BootState, MotionMode, MotionPreferences, StartupMotionMode,
    boot_duration_and_easing,
};
use terminalvision::app::actions::Action;
use terminalvision::app::modes::Mode;
use terminalvision::app::state::{ActivePane, App};
use terminalvision::input::keyboard::map_key_with_platform;
use terminalvision::input::mouse::{MouseTracker, handle_mouse_event_at};
use terminalvision::input::platform::Platform;
use terminalvision::ui::theme::ThemeId;

fn create_test_dir(name: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("tv-boot-{name}-"))
        .tempdir()
        .expect("Failed to create tempdir")
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

// -----------------------------------------------------------------------------
// Test 1: Boot state machine phases
// -----------------------------------------------------------------------------
#[test]
fn test_01_boot_state_machine_phases() {
    // Cinematic mode phase boundaries
    assert_eq!(
        BootPhase::from_progress(0.00, StartupMotionMode::Cinematic),
        BootPhase::Wake
    );
    assert_eq!(
        BootPhase::from_progress(0.10, StartupMotionMode::Cinematic),
        BootPhase::Wake
    );
    assert_eq!(
        BootPhase::from_progress(0.25, StartupMotionMode::Cinematic),
        BootPhase::Identity
    );
    assert_eq!(
        BootPhase::from_progress(0.50, StartupMotionMode::Cinematic),
        BootPhase::SystemReadiness
    );
    assert_eq!(
        BootPhase::from_progress(0.70, StartupMotionMode::Cinematic),
        BootPhase::ProjectAwareness
    );
    assert_eq!(
        BootPhase::from_progress(0.90, StartupMotionMode::Cinematic),
        BootPhase::VisionPulse
    );
    assert_eq!(
        BootPhase::from_progress(1.00, StartupMotionMode::Cinematic),
        BootPhase::Ready
    );

    // Minimal mode phase boundaries
    assert_eq!(
        BootPhase::from_progress(0.00, StartupMotionMode::Minimal),
        BootPhase::SystemReadiness
    );
    assert_eq!(
        BootPhase::from_progress(0.60, StartupMotionMode::Minimal),
        BootPhase::ProjectAwareness
    );
    assert_eq!(
        BootPhase::from_progress(0.90, StartupMotionMode::Minimal),
        BootPhase::VisionPulse
    );
    assert_eq!(
        BootPhase::from_progress(1.00, StartupMotionMode::Minimal),
        BootPhase::Ready
    );

    // Local progress calculations
    let local_wake = BootPhase::Wake.local_progress(0.10, StartupMotionMode::Cinematic);
    assert!((local_wake - 0.50).abs() < 0.01);

    let local_ready = BootPhase::Ready.local_progress(1.00, StartupMotionMode::Cinematic);
    assert_eq!(local_ready, 1.0);
}

// -----------------------------------------------------------------------------
// Test 2: State transitions in Cinematic mode with ManualClock
// -----------------------------------------------------------------------------
#[test]
fn test_02_cinematic_mode_startup_duration() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let prefs = MotionPreferences {
        startup: StartupMotionMode::Cinematic,
        mode: MotionMode::Full,
        ..Default::default()
    };

    let mut engine = AnimationEngine::new(clock.clone(), prefs);
    let (duration, easing) =
        boot_duration_and_easing(prefs.startup, prefs.mode).expect("Duration should be present");

    let id = engine.start(AnimationTag::VisionBoot, duration, easing);
    assert!(engine.is_animating());

    // 0ms -> Wake
    let p0 = engine.progress(id).unwrap();
    assert_eq!(
        BootPhase::from_progress(p0.eased, StartupMotionMode::Cinematic),
        BootPhase::Wake
    );

    // 500ms -> Identity / SystemReadiness
    clock.advance(Duration::from_millis(500));
    let p500 = engine.progress(id).unwrap();
    assert!(p500.eased > 0.15);

    // 1900ms -> Completed
    clock.advance(Duration::from_millis(1400));
    let p1900 = engine.progress(id).unwrap();
    assert_eq!(p1900.linear, 1.0);
    assert_eq!(
        BootPhase::from_progress(p1900.eased, StartupMotionMode::Cinematic),
        BootPhase::Ready
    );
}

// -----------------------------------------------------------------------------
// Test 3: Minimal mode startup duration
// -----------------------------------------------------------------------------
#[test]
fn test_03_minimal_mode_startup() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let prefs = MotionPreferences {
        startup: StartupMotionMode::Minimal,
        mode: MotionMode::Full,
        ..Default::default()
    };

    let mut engine = AnimationEngine::new(clock.clone(), prefs);
    let (duration, easing) =
        boot_duration_and_easing(prefs.startup, prefs.mode).expect("Duration should be present");

    let id = engine.start(AnimationTag::VisionBoot, duration, easing);
    assert!(engine.is_animating());

    // Advance 450ms -> should complete
    clock.advance(Duration::from_millis(450));
    let p = engine.progress(id).unwrap();
    assert_eq!(p.linear, 1.0);
    assert_eq!(
        BootPhase::from_progress(p.eased, StartupMotionMode::Minimal),
        BootPhase::Ready
    );
}

// -----------------------------------------------------------------------------
// Test 4: Off mode startup
// -----------------------------------------------------------------------------
#[test]
fn test_04_off_mode_startup() {
    let temp = create_test_dir("off");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    app.set_startup_motion_mode(StartupMotionMode::Off);
    app.start_boot();

    assert!(!app.is_booting());
    assert_eq!(app.mode(), Mode::Normal);
    assert!(app.boot_state().is_none());
}

// -----------------------------------------------------------------------------
// Test 5: Reduced motion mode
// -----------------------------------------------------------------------------
#[test]
fn test_05_reduced_motion_mode() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let prefs = MotionPreferences {
        startup: StartupMotionMode::Cinematic,
        mode: MotionMode::Reduced,
        ..Default::default()
    };

    let (duration, easing) =
        boot_duration_and_easing(prefs.startup, prefs.mode).expect("Duration should be present");
    assert_eq!(easing, Easing::Linear);
    assert!(duration <= Duration::from_millis(350));

    let mut engine = AnimationEngine::new(clock.clone(), prefs);
    let id = engine.start(AnimationTag::VisionBoot, duration, easing);

    clock.advance(Duration::from_millis(360));
    let p = engine.progress(id).unwrap();
    assert_eq!(p.linear, 1.0);
}

// -----------------------------------------------------------------------------
// Test 6: Cancellation and Esc skip
// -----------------------------------------------------------------------------
#[test]
fn test_06_cancellation_and_esc_skip() {
    let temp = create_test_dir("cancel");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    app.set_startup_motion_mode(StartupMotionMode::Cinematic);
    app.start_boot();
    assert!(app.is_booting());

    // Trigger Action::Cancel (e.g. Esc)
    app.handle_action(Action::Cancel);

    assert!(!app.is_booting());
    assert_eq!(app.mode(), Mode::Normal);
    assert!(app.boot_state().is_none());
}

// -----------------------------------------------------------------------------
// Test 7: Keyboard Esc key mapping in Mode::Boot
// -----------------------------------------------------------------------------
#[test]
fn test_07_keyboard_esc_key_mapping() {
    let esc_key = key(KeyCode::Esc, KeyModifiers::empty());
    let mapped = map_key_with_platform(esc_key, Mode::Boot, Platform::Linux);
    assert_eq!(mapped, Some(Action::Cancel));

    let space_key = key(KeyCode::Char(' '), KeyModifiers::empty());
    let mapped_space = map_key_with_platform(space_key, Mode::Boot, Platform::Linux);
    assert_eq!(mapped_space, Some(Action::Cancel));

    let ctrl_c = key(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let mapped_quit = map_key_with_platform(ctrl_c, Mode::Boot, Platform::Linux);
    assert_eq!(mapped_quit, Some(Action::Quit));
}

// -----------------------------------------------------------------------------
// Test 8: Mouse click skip
// -----------------------------------------------------------------------------
#[test]
fn test_08_mouse_click_skip() {
    let temp = create_test_dir("mouse_skip");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    app.set_startup_motion_mode(StartupMotionMode::Cinematic);
    app.start_boot();
    assert!(app.is_booting());

    let mut tracker = MouseTracker::new();
    let mouse_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 20,
        row: 10,
        modifiers: KeyModifiers::empty(),
    };

    let term_area = ratatui::layout::Rect::new(0, 0, 80, 24);
    handle_mouse_event_at(
        mouse_down,
        &mut app,
        term_area,
        &mut tracker,
        Instant::now(),
    );

    assert!(!app.is_booting());
    assert_eq!(app.mode(), Mode::Normal);
}

// -----------------------------------------------------------------------------
// Test 9: No-project environment
// -----------------------------------------------------------------------------
#[test]
fn test_09_no_project_environment() {
    let temp = create_test_dir("no_project");
    fs::write(temp.path().join("file1.txt"), "hello").unwrap();
    fs::write(temp.path().join("file2.txt"), "world").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_startup_motion_mode(StartupMotionMode::Cinematic);
    app.start_boot();

    let boot_state = app.boot_state().expect("BootState should be present");
    assert!(!boot_state.has_project());
    assert_eq!(boot_state.filesystem_entry_count, 2);
    assert!(boot_state.filesystem_ready);
    assert_eq!(boot_state.git_branch, None);
}

// -----------------------------------------------------------------------------
// Test 10: Rust project detection
// -----------------------------------------------------------------------------
#[test]
fn test_10_rust_project_detection() {
    let temp = create_test_dir("rust_proj");
    fs::write(
        temp.path().join("Cargo.toml"),
        r#"[package]
name = "my_rust_service"
version = "0.1.0"
"#,
    )
    .unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src").join("main.rs"), "fn main() {}").unwrap();
    fs::create_dir_all(temp.path().join("tests")).unwrap();
    fs::write(temp.path().join("tests").join("test.rs"), "// test").unwrap();
    fs::write(temp.path().join("README.md"), "# Service").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_startup_motion_mode(StartupMotionMode::Cinematic);
    app.start_boot();

    let boot_state = app.boot_state().expect("BootState should be present");
    assert!(boot_state.has_project());
    assert_eq!(boot_state.project_type.as_deref(), Some("Rust"));
    assert!(
        boot_state
            .project_name
            .as_ref()
            .map(|n| n.contains("rust_proj"))
            .unwrap_or(false)
    );

    let items: Vec<&str> = boot_state
        .project_structure_items
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert!(items.contains(&"src/"));
    assert!(items.contains(&"tests/"));
    assert!(items.contains(&"Cargo.toml"));
    assert!(items.contains(&"README.md"));
}

// -----------------------------------------------------------------------------
// Test 11: Git project and branch awareness
// -----------------------------------------------------------------------------
#[test]
fn test_11_git_project_and_branch_awareness() {
    let temp = create_test_dir("git_proj");
    fs::create_dir_all(temp.path().join(".git")).unwrap();
    fs::write(
        temp.path().join(".git").join("HEAD"),
        "ref: refs/heads/feature-vision\n",
    )
    .unwrap();
    fs::write(temp.path().join("main.py"), "print('hello')").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_startup_motion_mode(StartupMotionMode::Cinematic);
    app.start_boot();

    let boot_state = app.boot_state().expect("BootState should be present");
    assert_eq!(boot_state.git_branch.as_deref(), Some("feature-vision"));
}

// -----------------------------------------------------------------------------
// Test 12: Project without Git
// -----------------------------------------------------------------------------
#[test]
fn test_12_project_without_git() {
    let temp = create_test_dir("no_git_proj");
    fs::write(temp.path().join("package.json"), r#"{"name": "web-app"}"#).unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_startup_motion_mode(StartupMotionMode::Cinematic);
    app.start_boot();

    let boot_state = app.boot_state().expect("BootState should be present");
    assert!(boot_state.has_project());
    assert_eq!(boot_state.git_branch, None);
}

// -----------------------------------------------------------------------------
// Test 13: Initialization failure fallback
// -----------------------------------------------------------------------------
#[test]
fn test_13_initialization_failure_fallback() {
    let boot_state = BootState::new(
        StartupMotionMode::Cinematic,
        MotionMode::Full,
        false,
        PathBuf::from("/nonexistent/path"),
        0,
        false,
        true,
        None,
        None,
        Vec::new(),
        Vec::new(),
        None,
        false,
        Some("Filesystem permission denied".to_string()),
    );

    assert!(!boot_state.filesystem_ready);
    assert!(!boot_state.terminal_ready);
    assert_eq!(
        boot_state.error_notice.as_deref(),
        Some("Filesystem permission denied")
    );
}

// -----------------------------------------------------------------------------
// Test 14: Very small terminal rendering safety
// -----------------------------------------------------------------------------
#[test]
fn test_14_very_small_terminal_rendering() {
    let temp = create_test_dir("small_term");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_startup_motion_mode(StartupMotionMode::Cinematic);
    app.start_boot();

    let sizes = [(40, 10), (50, 12), (60, 14), (80, 24), (120, 30)];

    for (cols, rows) in sizes {
        let backend = TestBackend::new(cols, rows);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                terminalvision::ui::render(f, &app);
            })
            .expect("Render should not panic on small terminal");
    }
}

// -----------------------------------------------------------------------------
// Test 15: Terminal resize during boot
// -----------------------------------------------------------------------------
#[test]
fn test_15_terminal_resize_during_boot() {
    let temp = create_test_dir("resize_boot");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_startup_motion_mode(StartupMotionMode::Cinematic);
    app.start_boot();

    // Initial frame at 80x24
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| terminalvision::ui::render(f, &app))
        .unwrap();

    // Advance and resize to 160x40
    let backend_large = TestBackend::new(160, 40);
    let mut terminal_large = Terminal::new(backend_large).unwrap();
    terminal_large
        .draw(|f| terminalvision::ui::render(f, &app))
        .unwrap();

    // Advance and resize to compact 50x12
    let backend_compact = TestBackend::new(50, 12);
    let mut terminal_compact = Terminal::new(backend_compact).unwrap();
    terminal_compact
        .draw(|f| terminalvision::ui::render(f, &app))
        .unwrap();
}

// -----------------------------------------------------------------------------
// Test 16: Unicode and ASCII fallback
// -----------------------------------------------------------------------------
#[test]
fn test_16_unicode_and_ascii_fallback() {
    let temp = create_test_dir("symbols_fallback");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_startup_motion_mode(StartupMotionMode::Cinematic);
    app.start_boot();

    // Render with standard theme (Unicode symbols)
    app.set_active_theme(ThemeId::TerminalVision);
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| terminalvision::ui::render(f, &app))
        .unwrap();

    // Render with Monochrome theme
    app.set_active_theme(ThemeId::Monochrome);
    let backend_mono = TestBackend::new(80, 24);
    let mut terminal_mono = Terminal::new(backend_mono).unwrap();
    terminal_mono
        .draw(|f| terminalvision::ui::render(f, &app))
        .unwrap();
}

// -----------------------------------------------------------------------------
// Test 17: Theme and color palette fallback
// -----------------------------------------------------------------------------
#[test]
fn test_17_theme_and_color_palette_fallback() {
    let temp = create_test_dir("theme_palette");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_startup_motion_mode(StartupMotionMode::Cinematic);
    app.start_boot();

    for theme_id in ThemeId::ALL {
        app.set_active_theme(theme_id);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| terminalvision::ui::render(f, &app))
            .unwrap();
    }
}

// -----------------------------------------------------------------------------
// Test 18: Final focus and interactive restoration
// -----------------------------------------------------------------------------
#[test]
fn test_18_final_focus_and_interactive_restoration() {
    let temp = create_test_dir("focus_restore");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_startup_motion_mode(StartupMotionMode::Cinematic);
    app.start_boot();
    assert_eq!(app.mode(), Mode::Boot);

    app.skip_boot();

    assert_eq!(app.mode(), Mode::Normal);
    assert_eq!(app.active_pane(), ActivePane::Left);
    assert!(
        app.pane(ActivePane::Left).selected_index().is_some()
            || app.pane(ActivePane::Left).entries().is_empty()
    );
}

// -----------------------------------------------------------------------------
// Test 19: No stale startup state after completion
// -----------------------------------------------------------------------------
#[test]
fn test_19_no_stale_startup_state_after_completion() {
    let temp = create_test_dir("no_stale");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.set_startup_motion_mode(StartupMotionMode::Cinematic);
    app.start_boot();

    app.finish_boot();

    assert_eq!(app.mode(), Mode::Normal);
    assert!(app.boot_state().is_none());
    assert!(!app.has_active_animations());
}

// -----------------------------------------------------------------------------
// Test 20: Boot lifecycle and phase evaluation
// -----------------------------------------------------------------------------
#[test]
fn test_20_boot_lifecycle_and_phase_evaluation() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let prefs = MotionPreferences {
        startup: StartupMotionMode::Cinematic,
        mode: MotionMode::Full,
        ..Default::default()
    };

    let mut engine = AnimationEngine::new(clock.clone(), prefs);
    let (duration, easing) =
        boot_duration_and_easing(prefs.startup, prefs.mode).expect("Duration should be present");

    let id = engine.start(AnimationTag::VisionBoot, duration, easing);

    // Phase 1: Wake (0ms)
    let p1 = engine.progress(id).unwrap();
    assert_eq!(
        BootPhase::from_progress(p1.eased, StartupMotionMode::Cinematic),
        BootPhase::Wake
    );

    // Phase 2: Identity (600ms)
    clock.advance(Duration::from_millis(600));
    let p2 = engine.progress(id).unwrap();
    assert_eq!(
        BootPhase::from_progress(p2.eased, StartupMotionMode::Cinematic),
        BootPhase::Identity
    );

    // Phase 3: SystemReadiness (350ms -> 950ms total)
    clock.advance(Duration::from_millis(350));
    let p3 = engine.progress(id).unwrap();
    assert_eq!(
        BootPhase::from_progress(p3.eased, StartupMotionMode::Cinematic),
        BootPhase::SystemReadiness
    );

    // Phase 4: ProjectAwareness (300ms -> 1250ms total)
    clock.advance(Duration::from_millis(300));
    let p4 = engine.progress(id).unwrap();
    assert_eq!(
        BootPhase::from_progress(p4.eased, StartupMotionMode::Cinematic),
        BootPhase::ProjectAwareness
    );

    // Phase 5: VisionPulse (300ms -> 1550ms total)
    clock.advance(Duration::from_millis(300));
    let p5 = engine.progress(id).unwrap();
    assert_eq!(
        BootPhase::from_progress(p5.eased, StartupMotionMode::Cinematic),
        BootPhase::VisionPulse
    );

    // Phase 6: Ready (300ms -> 1850ms total)
    clock.advance(Duration::from_millis(300));
    let p6 = engine.progress(id).unwrap();
    assert_eq!(p6.linear, 1.0);
    assert_eq!(
        BootPhase::from_progress(p6.eased, StartupMotionMode::Cinematic),
        BootPhase::Ready
    );
}
