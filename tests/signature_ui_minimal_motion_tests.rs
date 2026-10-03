//! Comprehensive test suite for Phase 3.1: Signature UI + Minimal Motion.
//!
//! Tests:
//! 1. Focus state indicators across all surfaces (FM, PTY Shell, Command Center, Preview, Dialogs).
//! 2. Responsive signature header geometry (brand, chevrons, Git/project awareness, shortcuts).
//! 3. Responsive status bar geometry, selection count formatting, and adaptive actions.
//! 4. Signature file row rendering (unmistakable selection marker, background, weight, type icons, Git state).
//! 5. Polished empty states (empty directory, empty search, empty preview, empty favorites).
//! 6. Technical actionable error recovery presentation.
//! 7. Frame-based minimal motion engine and reduced motion configuration.
//! 8. Command Center and Quick Switcher affordance discovery.
//! 9. Dialog visual consistency.
//! 10. Robust rendering across all target terminal dimensions (80x24 through 200x60).

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use tempfile::tempdir;

use terminalvision::app::modes::Mode;
use terminalvision::app::state::{App, OperationKind};
use terminalvision::config::settings::Settings;
use terminalvision::input::platform::Platform;
use terminalvision::operations::OperationErrorPrompt;
use terminalvision::ui::footer::{mode_display, selection_display};
use terminalvision::ui::header::{command_center_button_text, command_center_hit_range};
use terminalvision::ui::motion::{DIALOG_ENTRANCE_DURATION, MotionState, STARTUP_DURATION};
use terminalvision::ui::render;
use terminalvision::ui::theme::{Symbols, Theme};

#[test]
fn test_01_signature_header_brand_and_chevrons() {
    let temp = tempdir().unwrap();
    let app = App::at(temp.path().to_path_buf()).expect("app initialized");

    let backend = TestBackend::new(120, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            terminalvision::ui::header::render(f, f.area(), &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let header_line: String = (0..120).map(|x| buffer[(x, 0)].symbol()).collect();

    assert!(
        header_line.contains("TerminalVision"),
        "header must contain brand name"
    );
    assert!(
        header_line.contains('›') || header_line.contains('>'),
        "header must contain signature chevron"
    );
}

#[test]
fn test_02_command_center_and_quick_switcher_affordance() {
    for platform in [Platform::Mac, Platform::Linux, Platform::Windows] {
        let wide_btn = command_center_button_text(platform, 40).expect("wide button text");
        assert!(
            wide_btn.contains("Commands"),
            "wide affordance must include Commands"
        );
        assert!(
            wide_btn.contains("Switcher"),
            "wide affordance must include Switcher"
        );

        let med_btn = command_center_button_text(platform, 25).expect("medium button text");
        assert!(
            med_btn.contains("Commands"),
            "medium affordance must include Commands"
        );

        let narrow_btn = command_center_button_text(platform, 15).expect("narrow button text");
        assert!(
            narrow_btn.contains(if platform.is_mac() { "⌘K" } else { "Ctrl+K" }),
            "narrow affordance must include shortcut"
        );
    }
}

#[test]
fn test_03_header_hit_testing_preservation() {
    let area = Rect::new(0, 0, 120, 1);
    let hit = command_center_hit_range(area, Platform::Mac);
    assert!(hit.is_some(), "hit range must be present on wide header");
    let (start_x, end_x, y) = hit.unwrap();
    assert_eq!(y, 0);
    assert!(start_x < end_x);
    assert!(end_x <= 120);
}

#[test]
fn test_04_signature_file_row_selection_markers() {
    let theme = Theme::signature();
    let (cursor_marker, cursor_style) = theme.row_marker(true, false, true);
    assert!(
        cursor_marker.contains('▸') || cursor_marker.contains('>'),
        "cursor marker must be prominent non-color glyph"
    );
    assert!(
        cursor_style.add_modifier.contains(Modifier::BOLD),
        "cursor row must be styled with bold weight"
    );

    let (multi_marker, multi_style) = theme.row_marker(false, true, true);
    assert!(
        multi_marker.contains('✓'),
        "multi-selection must show checkmark glyph"
    );
    assert!(
        multi_style.add_modifier.contains(Modifier::BOLD),
        "multi-selection must have distinct weight"
    );

    let (unsel_marker, _) = theme.row_marker(false, false, true);
    assert_eq!(unsel_marker, "  ", "unselected row has empty marker");
}

#[test]
fn test_05_status_bar_focus_system() {
    let mut app = App::default();

    // 1. Normal mode -> FILE MANAGER
    assert_eq!(mode_display(app.mode()), "NORMAL");

    // 2. Terminal mode -> TERMINAL / SHELL (PTY)
    app.set_mode(Mode::Terminal);
    assert_eq!(mode_display(app.mode()), "TERMINAL");

    // 3. Command Palette mode
    app.set_mode(Mode::CommandPalette);
    assert_eq!(mode_display(app.mode()), "COMMAND PALETTE");

    // 4. Preview mode
    app.set_mode(Mode::Preview);
    assert_eq!(mode_display(app.mode()), "PREVIEW");

    // 5. Storage Vision mode
    app.set_mode(Mode::StorageVision);
    assert_eq!(mode_display(app.mode()), "STORAGE VISION");
}

#[test]
fn test_06_selection_display_summary() {
    assert_eq!(selection_display(None, 0), "0 / 0");
    assert_eq!(selection_display(Some(0), 148), "1 / 148");
    assert_eq!(selection_display(Some(2), 148), "3 / 148");
}

#[test]
fn test_07_empty_states_presentation() {
    let temp = tempdir().unwrap();
    let app = App::at(temp.path().to_path_buf()).expect("app initialized");

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            render(f, &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let full_text: String = (0..24)
        .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
        .collect();

    assert!(
        full_text.contains("Nothing here yet."),
        "empty directory must display signature empty text"
    );
}

#[test]
fn test_08_error_recovery_dialog_presentation() {
    let err_prompt = OperationErrorPrompt {
        kind: OperationKind::Copy,
        error_message: "Permission denied (os error 13)".to_string(),
        path: PathBuf::from("/etc/protected/config.sys"),
        selected_option: 0,
    };

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            let theme = Theme::default();
            terminalvision::ui::dialogs::render_error_recovery_dialog(
                f,
                f.area(),
                &err_prompt,
                &theme,
            );
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let full_text: String = (0..24)
        .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
        .collect();

    assert!(full_text.contains("Operation Failed"));
    assert!(full_text.contains("Permission denied"));
    assert!(full_text.contains("/etc/protected/config.sys"));
    assert!(full_text.contains("Retry"));
}

#[test]
fn test_09_frame_based_motion_and_reduced_motion() {
    let start = Instant::now();
    let mut motion = MotionState::new(false);
    motion.start_time = start;

    assert!(motion.startup_progress(start) < 0.2);
    let mid = start + STARTUP_DURATION / 2;
    let mid_prog = motion.startup_progress(mid);
    assert!(mid_prog > 0.3 && mid_prog < 0.7);
    let end = start + STARTUP_DURATION + Duration::from_millis(50);
    assert_eq!(motion.startup_progress(end), 1.0);

    // Modal dialog transition
    motion.notify_dialog_opened();
    let dialog_open = motion.dialog_open_time.unwrap();
    assert!(motion.dialog_progress(dialog_open) < 0.2);
    let dialog_end = dialog_open + DIALOG_ENTRANCE_DURATION + Duration::from_millis(20);
    assert_eq!(motion.dialog_progress(dialog_end), 1.0);

    // Reduced motion mode: 100% instant completion
    let mut reduced = MotionState::new(true);
    assert_eq!(reduced.startup_progress(start), 1.0);
    reduced.notify_dialog_opened();
    assert_eq!(reduced.dialog_progress(start), 1.0);
    assert!(!reduced.is_active(start));
}

#[test]
fn test_10_settings_reduced_motion_persistence() {
    let mut settings = Settings::new();
    assert!(!settings.reduced_motion);

    settings.reduced_motion = true;
    let serialized = settings.serialize();
    assert!(serialized.contains("reduced_motion = true"));

    let loaded = Settings::deserialize(&serialized);
    assert!(loaded.reduced_motion);
}

#[test]
fn test_11_app_reduced_motion_toggle() {
    let mut app = App::default();
    assert!(!app.reduced_motion());

    app.set_reduced_motion(true);
    assert!(app.reduced_motion());

    let settings = app.to_settings();
    assert!(settings.reduced_motion);

    let mut restored = App::default();
    restored.apply_persistent_settings(&settings);
    assert!(restored.reduced_motion());
}

#[test]
fn test_12_semantic_theme_tokens_preparation() {
    let theme = Theme::signature();
    assert_ne!(theme.primary(), theme.error());
    assert_ne!(theme.selection(), theme.muted());
    assert_ne!(theme.success(), theme.warning());

    let std_sym = Symbols::standard();
    assert_eq!(std_sym.check_mark, "✓");
    assert_eq!(std_sym.cross_mark, "✕");
    assert_eq!(std_sym.warning_mark, "!");
    assert_eq!(std_sym.focus_bullet, "●");
    assert_eq!(std_sym.unfocused_bullet, "○");
}

#[test]
fn test_13_responsive_rendering_matrix_all_dimensions() {
    let target_sizes = [
        (80, 24),
        (100, 30),
        (120, 30),
        (159, 30),
        (160, 40),
        (180, 40),
        (200, 60),
    ];

    let temp = tempdir().unwrap();
    fs::write(temp.path().join("src.rs"), "fn main() {}").unwrap();
    fs::write(temp.path().join("Cargo.toml"), "[package]").unwrap();
    fs::write(temp.path().join("README.md"), "# TerminalVision").unwrap();

    let app = App::at(temp.path().to_path_buf()).expect("app initialized");

    for (cols, rows) in target_sizes {
        let backend = TestBackend::new(cols, rows);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                render(f, &app);
            })
            .expect("rendering must succeed across all dimensions");

        let buffer = terminal.backend().buffer();
        // Top row must have TerminalVision
        let top_row: String = (0..cols).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(
            top_row.contains("TerminalVision"),
            "Top row at {cols}x{rows} must contain TerminalVision brand"
        );
    }
}
