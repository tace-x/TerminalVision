//! The user interface layer.
//!
//! Renders the application state into terminal geometry using Ratatui.
//! The UI layer only reads prepared application state and never performs
//! filesystem operations or mutates domain models.

pub mod boot;
pub mod dialogs;
pub mod file_list;
pub mod footer;
pub mod header;
pub mod motion;
pub mod panes;
pub mod preview;
pub mod terminal;
pub mod theme;

pub use motion::MotionState;
pub use theme::{NotificationLevel, Spacing, Symbols, Theme, ThemeId, ThemePalette, ThemeRegistry};

use std::path::Path;

use ratatui::Frame;
use ratatui::text::Span;

use crate::app::state::App;
use crate::layout::geometry::ScreenLayout;

/// Computes the terminal display width of `text` in columns.
///
/// Respects multi-byte Unicode characters and double-width glyphs.
pub fn display_width(text: &str) -> usize {
    Span::raw(text).width()
}

/// Truncates `text` from the end so its display width does not exceed `max_width`.
///
/// If truncated, appends the single-column ellipsis `'…'` when `max_width >= 1`.
pub fn truncate_to_width(text: &str, max_width: usize) -> String {
    let width = display_width(text);
    if width <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }

    let target_width = max_width.saturating_sub(1);
    let mut current_width = 0;
    let mut end_index = 0;

    for (idx, ch) in text.char_indices() {
        let ch_str = &text[idx..idx + ch.len_utf8()];
        let ch_width = display_width(ch_str);
        if current_width + ch_width > target_width {
            break;
        }
        current_width += ch_width;
        end_index = idx + ch.len_utf8();
    }

    let mut result = String::with_capacity(end_index + 3);
    result.push_str(&text[..end_index]);
    result.push('…');
    result
}

/// Intelligently truncates a filename so its display width does not exceed `max_width`.
///
/// When space permits, preserves the file extension (e.g. `very-important-file.rs` -> `very-important…rs`).
pub fn truncate_filename_to_width(name: &str, max_width: usize) -> String {
    let width = display_width(name);
    if width <= max_width {
        return name.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }

    let path = Path::new(name);
    let ext_str = path.extension().and_then(|e| e.to_str());

    if let Some(ext) = ext_str
        && !name.starts_with('.')
        && !ext.is_empty()
    {
        let full_ext = format!(".{ext}");
        let ext_w = display_width(&full_ext);

        if max_width > ext_w + 1 {
            let stem_max = max_width.saturating_sub(ext_w + 1);
            let stem = &name[..name.len().saturating_sub(full_ext.len())];
            let truncated_stem = truncate_to_width(stem, stem_max.saturating_add(1));
            if truncated_stem.ends_with('…') {
                return format!("{truncated_stem}{ext}");
            } else {
                return format!("{truncated_stem}…{ext}");
            }
        }
    }

    truncate_to_width(name, max_width)
}

/// Truncates a filesystem path so that its display width does not exceed `max_width`.
///
/// Truncates leading parent components first (prepending `'…'`) so that the most
/// immediate directory or filename remains visible.
pub fn truncate_path_to_width(path: &Path, max_width: usize) -> String {
    let text = path.display().to_string();
    let width = display_width(&text);
    if width <= max_width {
        return text;
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }

    let target_width = max_width.saturating_sub(1);
    let mut current_width = 0;
    let mut start_index = text.len();

    for (idx, ch) in text.char_indices().rev() {
        let ch_str = &text[idx..idx + ch.len_utf8()];
        let ch_width = display_width(ch_str);
        if current_width + ch_width > target_width {
            break;
        }
        current_width += ch_width;
        start_index = idx;
    }

    let mut result = String::with_capacity(3 + (text.len() - start_index));
    result.push('…');
    result.push_str(&text[start_index..]);
    result
}

/// Renders the complete application interface into `frame`.
///
/// Driven entirely by the layout rectangles derived by `ScreenLayout::calculate`.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    // If currently running Vision Boot sequence, render the startup experience
    if let (crate::app::modes::Mode::Boot, Some(boot_state)) = (app.mode(), app.boot_state()) {
        boot::render(frame, area, app, boot_state, app.boot_progress());
        return;
    }

    let layout = ScreenLayout::calculate(area);

    if layout.header().height > 0 && layout.header().width > 0 {
        header::render(frame, layout.header(), app);
    }

    panes::render(frame, layout.main(), app);

    if layout.terminal().height > 0 && layout.terminal().width > 0 {
        let is_terminal_focused = app.mode() == crate::app::modes::Mode::Terminal;
        terminal::render(frame, layout.terminal(), app, is_terminal_focused);
    }

    if layout.footer().height > 0 && layout.footer().width > 0 {
        footer::render(frame, layout.footer(), app);
    }

    dialogs::render(frame, area, app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn display_width_computes_ascii_and_unicode() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width(""), 0);
        assert_eq!(display_width("中文"), 4);
    }

    #[test]
    fn truncate_to_width_handles_short_and_long_strings() {
        assert_eq!(truncate_to_width("hello", 10), "hello");
        assert_eq!(truncate_to_width("hello world", 5), "hell…");
        assert_eq!(truncate_to_width("hello", 1), "…");
        assert_eq!(truncate_to_width("hello", 0), "");
    }

    #[test]
    fn truncate_to_width_handles_wide_characters() {
        assert_eq!(truncate_to_width("你好世界", 5), "你好…");
        assert_eq!(truncate_to_width("你好世界", 4), "你…");
    }

    #[test]
    fn truncate_path_to_width_keeps_tail_of_path() {
        let path = PathBuf::from("/users/immanuelmelbin/documents/projects/terminalvision");
        let truncated = truncate_path_to_width(&path, 25);
        assert!(truncated.starts_with('…'));
        assert!(truncated.ends_with("terminalvision"));
        assert!(display_width(&truncated) <= 25);
    }

    #[test]
    fn truncate_filename_to_width_preserves_extensions() {
        let name = "very-important-project-file.rs";
        let res_18 = truncate_filename_to_width(name, 18);
        assert_eq!(res_18, "very-important…rs");
        assert!(display_width(&res_18) <= 18);

        let res_6 = truncate_filename_to_width(name, 6);
        assert_eq!(res_6, "ve…rs");
        assert!(display_width(&res_6) <= 6);

        let res_exact = truncate_filename_to_width(name, display_width(name));
        assert_eq!(res_exact, name);

        let res_no_ext = truncate_filename_to_width("some-long-directory-name", 10);
        assert_eq!(res_no_ext, "some-long…");
        assert!(display_width(&res_no_ext) <= 10);

        let res_unicode = truncate_filename_to_width("മലയാളം_പ്രൊജക്റ്റ്.rs", 10);
        assert!(display_width(&res_unicode) <= 10);
        assert!(res_unicode.ends_with("rs") || res_unicode.ends_with('…'));
    }

    // --- Phase 9.1 / 9.2 Verification Tests ---

    #[test]
    fn test_1_main_layout_renders_without_panic() {
        let sizes = [
            (80, 24),
            (100, 30),
            (120, 30),
            (159, 30),
            (160, 40),
            (180, 40),
            (200, 60),
        ];
        let app = App::default();

        for (cols, rows) in sizes {
            let backend = ratatui::backend::TestBackend::new(cols, rows);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| {
                    render(f, &app);
                })
                .unwrap();
        }
    }

    #[test]
    fn test_2_narrow_layout_renders_one_pane() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let app = App::default();

        terminal
            .draw(|f| {
                render(f, &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        // Row 0 is header, row 1 is top border of main pane
        let row_1: String = (0..80).map(|x| buffer[(x, 1)].symbol()).collect();
        assert!(
            row_1.contains("●"),
            "narrow layout must render the active pane with ● indicator"
        );
    }

    #[test]
    fn test_3_normal_layout_renders_two_panes() {
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let app = App::default();

        terminal
            .draw(|f| {
                render(f, &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        // Row 1 is the pane top border row. The active pane shows ●.
        let row_1: String = (0..100).map(|x| buffer[(x, 1)].symbol()).collect();
        assert!(
            row_1.contains("●"),
            "normal layout must show active pane ● indicator"
        );
        assert!(
            !row_1.contains("Preview"),
            "normal layout must not reserve preview"
        );
    }

    #[test]
    fn test_4_wide_layout_reserves_preview_region() {
        let backend = ratatui::backend::TestBackend::new(180, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let app = App::default();

        terminal
            .draw(|f| {
                render(f, &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let row_1: String = (0..180).map(|x| buffer[(x, 1)].symbol()).collect();
        assert!(row_1.contains("●"), "wide layout must show active pane ●");
        assert!(
            row_1.contains("Preview"),
            "wide layout must reserve preview region"
        );
    }

    #[test]
    fn test_5_active_pane_indicator_works() {
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = App::default();

        // Initially Left is active — its pane title row should have ●
        terminal
            .draw(|f| {
                render(f, &app);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        // In dual-pane mode at 100 cols, left pane occupies columns 0..50, right pane 50..100
        let left_row: String = (0..50).map(|x| buffer[(x, 1)].symbol()).collect();
        let right_row: String = (50..100).map(|x| buffer[(x, 1)].symbol()).collect();
        assert!(left_row.contains("●"), "left pane must show ● when active");
        assert!(
            !right_row.contains("●"),
            "right pane must not show ● when inactive"
        );

        // Switch to Right pane
        app.handle_action(crate::app::actions::Action::SwitchPane);
        terminal
            .draw(|f| {
                render(f, &app);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let left_row: String = (0..50).map(|x| buffer[(x, 1)].symbol()).collect();
        let right_row: String = (50..100).map(|x| buffer[(x, 1)].symbol()).collect();
        assert!(
            !left_row.contains("●"),
            "left pane must not show ● when inactive"
        );
        assert!(
            right_row.contains("●"),
            "right pane must show ● when active"
        );
    }

    #[test]
    fn test_6_long_path_handling_does_not_overflow() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = App::default();

        let long_path = PathBuf::from("/".repeat(10) + &"very_long_path_name/".repeat(30));
        let _ = app.open_in(crate::app::state::ActivePane::Left, long_path);

        terminal
            .draw(|f| {
                render(f, &app);
            })
            .unwrap();

        // Ensure buffer width is respected and cells within bounds
        let buffer = terminal.backend().buffer();
        for y in 0..24 {
            let row: String = (0..80).map(|x| buffer[(x, y)].symbol()).collect();
            assert_eq!(row.chars().count(), 80);
        }
    }

    #[test]
    fn test_7_unicode_path_handling_is_safe() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = App::default();

        let unicode_path = PathBuf::from("/🦀/üñîçødé/日本語/файл/test");
        let _ = app.open_in(crate::app::state::ActivePane::Left, unicode_path);

        terminal
            .draw(|f| {
                render(f, &app);
            })
            .unwrap();
    }

    #[test]
    fn test_8_tiny_terminal_dimensions_do_not_panic() {
        let tiny_sizes = [
            (0, 0),
            (1, 1),
            (5, 5),
            (10, 2),
            (20, 3),
            (80, 1),
            (80, 2),
            (3, 80),
        ];
        let app = App::default();

        for (cols, rows) in tiny_sizes {
            let backend = ratatui::backend::TestBackend::new(cols, rows);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| {
                    render(f, &app);
                })
                .unwrap();
        }
    }

    #[test]
    fn test_9_existing_application_state_remains_intact_after_rendering() {
        let mut app = App::default();
        app.handle_action(crate::app::actions::Action::MoveDown);
        let before = app.clone();

        let backend = ratatui::backend::TestBackend::new(120, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                render(f, &app);
            })
            .unwrap();

        assert_eq!(app, before, "rendering must never mutate application state");
    }

    #[test]
    fn test_10_rendering_does_not_perform_filesystem_operations() {
        let app = App::default();

        // Rendering with an empty/unreadable default path must not trigger errors or notices
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                render(f, &app);
            })
            .unwrap();

        assert!(
            !app.notification().is_active(),
            "rendering must not perform any filesystem calls or generate error notifications"
        );
    }

    #[test]
    fn test_terminal_size_stress_all_dimensions_and_all_modes() {
        use crate::app::modes::Mode;
        use crate::input::mouse::{hit_test_pane, hit_test_row};
        use ratatui::layout::Rect;

        let required_dimensions = [
            (1, 1),
            (2, 2),
            (10, 5),
            (20, 5),
            (40, 10),
            (79, 23),
            (80, 24),
            (81, 24),
            (99, 30),
            (100, 30),
            (101, 30),
            (119, 30),
            (120, 30),
            (159, 30),
            (160, 40),
            (161, 40),
            (199, 59),
            (200, 60),
            (240, 100),
        ];

        let all_modes = [
            Mode::Normal,
            Mode::Search,
            Mode::Preview,
            Mode::Create,
            Mode::Rename,
            Mode::Confirm,
            Mode::Bookmarks,
            Mode::CommandPalette,
            Mode::Help,
        ];

        for (cols, rows) in required_dimensions {
            let backend = ratatui::backend::TestBackend::new(cols, rows);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();

            for mode in all_modes {
                let mut app = App::default();
                app.set_mode(mode);
                if mode == Mode::Preview {
                    app.handle_action(crate::app::actions::Action::Preview);
                }

                // Rendering must not panic, overflow, or crash under any size or mode
                terminal
                    .draw(|f| {
                        render(f, &app);
                    })
                    .expect("render must succeed for all dimensions and modes");

                // Screen layout regions must be valid, non-negative, and within terminal area
                let term_area = Rect::new(0, 0, cols, rows);
                let layout = ScreenLayout::calculate(term_area);
                assert!(layout.header().x + layout.header().width <= cols);
                assert!(layout.header().y + layout.header().height <= rows);
                assert!(layout.footer().x + layout.footer().width <= cols);
                assert!(layout.footer().y + layout.footer().height <= rows);

                for (_, region_rect) in layout.main().regions() {
                    assert!(region_rect.x + region_rect.width <= cols);
                    assert!(region_rect.y + region_rect.height <= rows);
                }

                // Mouse hit-testing must never panic on in-bounds or boundary coordinates
                let test_coords = [
                    (0, 0),
                    (cols.saturating_sub(1), rows.saturating_sub(1)),
                    (cols / 2, rows / 2),
                    (cols, rows),
                    (cols.saturating_add(10), rows.saturating_add(10)),
                ];
                for (cx, cy) in test_coords {
                    let _ = hit_test_pane(cx, cy, term_area, app.active_pane());
                    let _ = hit_test_row(cx, cy, term_area, 0, 10);
                }
            }
        }
    }

    #[test]
    fn test_unicode_display_safety_and_alignment() {
        let malayalam = "മലയാളം പ്രോജക്റ്റ്";
        let japanese = "日本語のファイル名";
        let chinese = "简体中文目录";
        let accented = "résumé_über_señor_først.txt";
        let emoji = "📁_🦀_⚡_🔍_app";
        let combining = "e\u{0301}cole_cafe\u{0301}.rs";

        let sample_texts = [malayalam, japanese, chinese, accented, emoji, combining];

        for text in sample_texts {
            let width = display_width(text);
            assert!(width > 0, "display width of {text} must be > 0");

            for max_w in 0..=30 {
                let truncated = truncate_to_width(text, max_w);
                let trunc_w = display_width(&truncated);
                assert!(
                    trunc_w <= max_w,
                    "truncated width {} exceeds max_w {} for '{text}' -> '{truncated}'",
                    trunc_w,
                    max_w
                );
            }

            let path = PathBuf::from(format!("/home/user/projects/{text}/nested/file.txt"));
            for max_w in 0..=40 {
                let trunc_path = truncate_path_to_width(&path, max_w);
                let trunc_path_w = display_width(&trunc_path);
                assert!(
                    trunc_path_w <= max_w,
                    "truncated path width {} exceeds max_w {} for '{path:?}' -> '{trunc_path}'",
                    trunc_path_w,
                    max_w
                );
            }
        }
    }
}
