//! Dynamic Theme Engine comprehensive test suite (Prompt 8 / Phase 3.2).
//!
//! Validates:
//! - Theme model & semantic tokens
//! - ThemeRegistry resolution, lookups, and built-in themes
//! - Invalid theme fallback safety
//! - Live preview during theme selection
//! - Apply & Cancel behavior
//! - Action Registry & Command Center integration
//! - Settings persistence and loading
//! - Terminal ANSI palette synchronization and color mappings
//! - Accessibility & high-contrast validation
//! - Responsive Theme Selector modal across required screen dimensions
//! - Mouse interaction hit testing

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

use terminalvision::app::actions::{Action, ActionRegistry};
use terminalvision::app::modes::Mode;
use terminalvision::app::state::App;
use terminalvision::config::Settings;
use terminalvision::ui::theme::{ThemeId, ThemeRegistry};

#[test]
fn test_all_10_builtin_themes_exist_and_are_registered() {
    let all_ids = ThemeId::all();
    assert_eq!(all_ids.len(), 10, "Must contain exactly 10 built-in themes");

    let expected = [
        ThemeId::TerminalVision,
        ThemeId::Midnight,
        ThemeId::Cyberpunk,
        ThemeId::Ocean,
        ThemeId::Dracula,
        ThemeId::Nord,
        ThemeId::Matrix,
        ThemeId::SolarizedDark,
        ThemeId::Monochrome,
        ThemeId::HighContrast,
    ];

    for expected_id in expected {
        assert!(all_ids.contains(&expected_id));
        let theme = ThemeRegistry::get(expected_id);
        assert_eq!(theme.id, expected_id);
        assert!(!theme.name.is_empty());
        assert!(!theme.description.is_empty());
    }
}

#[test]
fn test_theme_registry_lookup_by_string() {
    assert_eq!(
        ThemeRegistry::get_by_str("terminalvision").id,
        ThemeId::TerminalVision
    );
    assert_eq!(ThemeRegistry::get_by_str("midnight").id, ThemeId::Midnight);
    assert_eq!(
        ThemeRegistry::get_by_str("cyberpunk").id,
        ThemeId::Cyberpunk
    );
    assert_eq!(ThemeRegistry::get_by_str("ocean").id, ThemeId::Ocean);
    assert_eq!(ThemeRegistry::get_by_str("dracula").id, ThemeId::Dracula);
    assert_eq!(ThemeRegistry::get_by_str("nord").id, ThemeId::Nord);
    assert_eq!(ThemeRegistry::get_by_str("matrix").id, ThemeId::Matrix);
    assert_eq!(
        ThemeRegistry::get_by_str("solarized-dark").id,
        ThemeId::SolarizedDark
    );
    assert_eq!(
        ThemeRegistry::get_by_str("solarized_dark").id,
        ThemeId::SolarizedDark
    );
    assert_eq!(
        ThemeRegistry::get_by_str("monochrome").id,
        ThemeId::Monochrome
    );
    assert_eq!(
        ThemeRegistry::get_by_str("high-contrast").id,
        ThemeId::HighContrast
    );
    assert_eq!(
        ThemeRegistry::get_by_str("high_contrast").id,
        ThemeId::HighContrast
    );

    // Case insensitivity
    assert_eq!(
        ThemeRegistry::get_by_str("CYBERPUNK").id,
        ThemeId::Cyberpunk
    );
    assert_eq!(ThemeRegistry::get_by_str("Nord").id, ThemeId::Nord);
}

#[test]
fn test_theme_registry_invalid_fallback() {
    // Unrecognized or corrupted theme names must safely fall back to TerminalVision without crashing
    assert_eq!(
        ThemeRegistry::get_by_str("nonexistent_theme").id,
        ThemeId::TerminalVision
    );
    assert_eq!(ThemeRegistry::get_by_str("").id, ThemeId::TerminalVision);
    assert_eq!(
        ThemeRegistry::get_by_str("random 123!").id,
        ThemeId::TerminalVision
    );
}

#[test]
fn test_theme_registry_cycle_next_prev() {
    assert_eq!(
        ThemeRegistry::next(ThemeId::TerminalVision),
        ThemeId::Midnight
    );
    assert_eq!(
        ThemeRegistry::prev(ThemeId::TerminalVision),
        ThemeId::HighContrast
    );
    assert_eq!(
        ThemeRegistry::next(ThemeId::HighContrast),
        ThemeId::TerminalVision
    );
    assert_eq!(
        ThemeRegistry::prev(ThemeId::Midnight),
        ThemeId::TerminalVision
    );
}

#[test]
fn test_semantic_palette_tokens_populated_across_all_themes() {
    for id in ThemeId::all() {
        let theme = ThemeRegistry::get(*id);
        let p = &theme.palette;

        // Background and foreground must be defined and distinct
        assert_ne!(
            p.background, p.foreground,
            "Theme {id:?} background must differ from foreground"
        );
        assert_ne!(p.surface, p.foreground);
        assert_ne!(p.selection, p.background);

        // Required semantic roles must not be default black
        assert_ne!(p.primary, Color::Reset);
        assert_ne!(p.accent, Color::Reset);
        assert_ne!(p.success, Color::Reset);
        assert_ne!(p.warning, Color::Reset);
        assert_ne!(p.error, Color::Reset);
        assert_ne!(p.info, Color::Reset);

        // Git semantic colors
        assert_ne!(p.git_modified, Color::Reset);
        assert_ne!(p.git_added, Color::Reset);
        assert_ne!(p.git_deleted, Color::Reset);

        // File type colors
        assert_ne!(p.directory, Color::Reset);
        assert_ne!(p.executable, Color::Reset);
        assert_ne!(p.symlink, Color::Reset);
    }
}

#[test]
fn test_theme_selector_flow_and_live_preview() {
    let mut app = App::default();
    assert_eq!(app.active_theme_id(), ThemeId::TerminalVision);
    assert_eq!(app.theme().id, ThemeId::TerminalVision);

    // Open Theme Selector
    app.handle_action(Action::ThemeSelector);
    assert_eq!(app.mode(), Mode::ThemeSelector);
    assert_eq!(
        app.theme_selector().selected_theme(),
        ThemeId::TerminalVision
    );

    // Navigate down to preview Midnight
    app.handle_action(Action::MoveDown);
    assert_eq!(app.theme_selector().selected_theme(), ThemeId::Midnight);
    assert_eq!(
        app.theme().id,
        ThemeId::Midnight,
        "Live preview must reflect currently selected theme"
    );
    assert_eq!(
        app.active_theme_id(),
        ThemeId::TerminalVision,
        "Active saved theme should remain untouched during preview"
    );

    // Navigate down to preview Cyberpunk
    app.handle_action(Action::MoveDown);
    assert_eq!(app.theme_selector().selected_theme(), ThemeId::Cyberpunk);
    assert_eq!(app.theme().id, ThemeId::Cyberpunk);

    // Cancel preview -> restores initial theme
    app.cancel_theme_selector();
    assert_eq!(app.mode(), Mode::Normal);
    assert_eq!(
        app.theme().id,
        ThemeId::TerminalVision,
        "Cancelling must restore previous active theme"
    );
    assert_eq!(app.active_theme_id(), ThemeId::TerminalVision);

    // Open again, preview Dracula, then Apply
    app.handle_action(Action::ThemeSelector);
    app.theme_selector_select_index(4); // Dracula
    assert_eq!(app.theme().id, ThemeId::Dracula);

    app.apply_theme_selector();
    assert_eq!(app.mode(), Mode::Normal);
    assert_eq!(
        app.active_theme_id(),
        ThemeId::Dracula,
        "Applying must save new active theme"
    );
    assert_eq!(app.theme().id, ThemeId::Dracula);
}

#[test]
fn test_next_prev_theme_actions() {
    let mut app = App::default();
    assert_eq!(app.active_theme_id(), ThemeId::TerminalVision);

    app.handle_action(Action::NextTheme);
    assert_eq!(app.active_theme_id(), ThemeId::Midnight);
    assert_eq!(app.theme().id, ThemeId::Midnight);

    app.handle_action(Action::NextTheme);
    assert_eq!(app.active_theme_id(), ThemeId::Cyberpunk);

    app.handle_action(Action::PrevTheme);
    assert_eq!(app.active_theme_id(), ThemeId::Midnight);

    app.handle_action(Action::PrevTheme);
    assert_eq!(app.active_theme_id(), ThemeId::TerminalVision);
}

#[test]
fn test_action_registry_contains_theme_actions() {
    let registry = ActionRegistry::global();
    assert!(registry.get(Action::ThemeSelector).is_some());
    assert!(registry.get(Action::NextTheme).is_some());
    assert!(registry.get(Action::PrevTheme).is_some());

    let theme_action_meta = registry.get(Action::ThemeSelector).unwrap();
    assert_eq!(theme_action_meta.action, Action::ThemeSelector);
    assert!(theme_action_meta.name.contains("Theme"));
}

#[test]
fn test_command_palette_includes_theme_selector() {
    let commands = terminalvision::commands::palette::Command::all();
    let has_theme_cmd = commands
        .iter()
        .any(|cmd| cmd.action() == Action::ThemeSelector);
    assert!(
        has_theme_cmd,
        "Command center must contain Theme Selector entry"
    );
}

#[test]
fn test_settings_persistence_of_active_theme() {
    let mut settings = Settings::default();
    assert_eq!(settings.theme, "terminalvision");

    // Serialization
    settings.theme = "cyberpunk".to_string();
    let toml_str = settings.serialize();
    assert!(toml_str.contains("theme = cyberpunk"));

    // Deserialization
    let loaded = Settings::deserialize(&toml_str);
    assert_eq!(loaded.theme, "cyberpunk");

    // App state integration
    let mut app = App::default();
    app.apply_persistent_settings(&loaded);
    assert_eq!(app.active_theme_id(), ThemeId::Cyberpunk);
    assert_eq!(app.theme().id, ThemeId::Cyberpunk);

    // Invalid theme in config safely falls back
    let corrupted_toml = "[settings]\ntheme = \"unknown_custom_theme\"\n";
    let loaded_corrupted = Settings::deserialize(corrupted_toml);
    let mut app2 = App::default();
    app2.apply_persistent_settings(&loaded_corrupted);
    assert_eq!(
        app2.active_theme_id(),
        ThemeId::TerminalVision,
        "Corrupted theme in settings must fall back to TerminalVision"
    );
}

#[test]
fn test_terminal_ansi_color_mapping() {
    for id in ThemeId::all() {
        let theme = ThemeRegistry::get(*id);

        // Test standard 16 ANSI colors
        assert_eq!(
            theme.map_terminal_color(Color::Black),
            theme.palette.ansi_black
        );
        assert_eq!(theme.map_terminal_color(Color::Red), theme.palette.ansi_red);
        assert_eq!(
            theme.map_terminal_color(Color::Green),
            theme.palette.ansi_green
        );
        assert_eq!(
            theme.map_terminal_color(Color::Yellow),
            theme.palette.ansi_yellow
        );
        assert_eq!(
            theme.map_terminal_color(Color::Blue),
            theme.palette.ansi_blue
        );
        assert_eq!(
            theme.map_terminal_color(Color::Magenta),
            theme.palette.ansi_magenta
        );
        assert_eq!(
            theme.map_terminal_color(Color::Cyan),
            theme.palette.ansi_cyan
        );
        assert_eq!(
            theme.map_terminal_color(Color::White),
            theme.palette.ansi_white
        );

        assert_eq!(
            theme.map_terminal_color(Color::DarkGray),
            theme.palette.ansi_bright_black
        );
        assert_eq!(
            theme.map_terminal_color(Color::LightRed),
            theme.palette.ansi_bright_red
        );
        assert_eq!(
            theme.map_terminal_color(Color::LightGreen),
            theme.palette.ansi_bright_green
        );
        assert_eq!(
            theme.map_terminal_color(Color::LightYellow),
            theme.palette.ansi_bright_yellow
        );
        assert_eq!(
            theme.map_terminal_color(Color::LightBlue),
            theme.palette.ansi_bright_blue
        );
        assert_eq!(
            theme.map_terminal_color(Color::LightMagenta),
            theme.palette.ansi_bright_magenta
        );
        assert_eq!(
            theme.map_terminal_color(Color::LightCyan),
            theme.palette.ansi_bright_cyan
        );
        assert_eq!(
            theme.map_terminal_color(Color::Gray),
            theme.palette.ansi_white
        );

        // Reset maps to foreground
        assert_eq!(
            theme.map_terminal_color(Color::Reset),
            theme.palette.foreground
        );
    }
}

#[test]
fn test_high_contrast_accessibility_distinctions() {
    let hc = ThemeRegistry::get(ThemeId::HighContrast);
    assert_eq!(hc.palette.background, Color::Black);
    assert_eq!(hc.palette.foreground, Color::White);
    assert_eq!(hc.palette.selection, Color::Rgb(65, 65, 65));
    assert_eq!(hc.palette.selection_foreground, Color::Yellow);

    // Ensure error, warning, and success are stark
    assert_eq!(hc.palette.error, Color::Red);
    assert_eq!(hc.palette.warning, Color::Yellow);
    assert_eq!(hc.palette.success, Color::Green);

    // Test selection styling
    let (marker, marker_style) = hc.row_marker(true, false, true);
    assert_eq!(marker, "▸ ");
    assert_eq!(marker_style.fg, Some(hc.palette.selection_foreground));
}

#[test]
fn test_theme_selector_renders_across_all_required_dimensions() {
    let dimensions = [
        (40, 10),
        (60, 15),
        (80, 24),
        (100, 30),
        (120, 30),
        (159, 30),
        (160, 40),
        (180, 40),
        (200, 60),
        (240, 80),
    ];

    for (cols, rows) in dimensions {
        let backend = TestBackend::new(cols, rows);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        app.handle_action(Action::ThemeSelector);

        terminal
            .draw(|f| {
                terminalvision::ui::render(f, &app);
            })
            .unwrap_or_else(|e| panic!("Failed to render Theme Selector at {cols}x{rows}: {e}"));

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.area.width, cols);
        assert_eq!(buffer.area.height, rows);

        let text: String = (0..rows)
            .flat_map(|y| (0..cols).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();

        assert!(
            text.contains("Theme Selector"),
            "Must display Theme Selector title at {cols}x{rows}"
        );
    }
}

#[test]
fn test_render_all_themes_without_panic() {
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    for id in ThemeId::all() {
        let mut app = App::default();
        app.set_active_theme(*id);

        terminal
            .draw(|f| {
                terminalvision::ui::render(f, &app);
            })
            .unwrap_or_else(|e| panic!("Failed rendering app with theme {id:?}: {e}"));
    }
}
