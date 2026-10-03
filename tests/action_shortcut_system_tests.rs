//! Dedicated test suite for the Universal Action + Cross-Platform Shortcut System.
//!
//! Tests the 10 required architectural guarantees:
//! 1. Action registry creation
//! 2. Shortcut lookup
//! 3. macOS modifier mapping
//! 4. Windows/Linux modifier mapping
//! 5. Context filtering
//! 6. Conflict detection
//! 7. Shortcut formatting
//! 8. Show Shortcuts generation
//! 9. Action dispatch
//! 10. Invalid/unavailable action handling

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::fs;
use tempfile::tempdir;

use terminalvision::app::actions::{Action, ActionCategory, ActionContext, ActionRegistry};
use terminalvision::app::modes::Mode;
use terminalvision::app::state::App;
use terminalvision::input::InputEvent;
use terminalvision::input::keyboard::{map_key_event_with_platform, map_key_with_platform};
use terminalvision::input::platform::Platform;
use terminalvision::input::shortcut::{KeyChord, ShortcutBinding, ShortcutRegistry};

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn plain_press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

// ---------------------------------------------------------------------------
// 1. Action Registry Creation
// ---------------------------------------------------------------------------
#[test]
fn test_1_action_registry_creation() {
    let registry = ActionRegistry::global();
    assert_eq!(registry.all().len(), Action::ALL.len());

    for action in Action::ALL {
        let meta = registry
            .get(action)
            .expect("Every action must have metadata");
        assert_eq!(meta.action, action);
        assert!(!meta.name.trim().is_empty());
        assert!(!meta.description.trim().is_empty());
    }

    let nav_actions = registry.by_category(ActionCategory::Navigation);
    assert!(!nav_actions.is_empty());
    let files_actions = registry.by_category(ActionCategory::Files);
    assert!(!files_actions.is_empty());
}

// ---------------------------------------------------------------------------
// 2. Shortcut Lookup
// ---------------------------------------------------------------------------
#[test]
fn test_2_shortcut_lookup() {
    let registry = ShortcutRegistry::global();
    let mac = Platform::Mac;

    // Normal navigation
    let enter = plain_press(KeyCode::Enter);
    assert_eq!(
        registry.lookup(&enter, ActionContext::FileManager, mac),
        Some(Action::Open)
    );

    let backspace = plain_press(KeyCode::Backspace);
    assert_eq!(
        registry.lookup(&backspace, ActionContext::FileManager, mac),
        Some(Action::GoParent)
    );

    let tab = plain_press(KeyCode::Tab);
    assert_eq!(
        registry.lookup(&tab, ActionContext::FileManager, mac),
        Some(Action::SwitchPane)
    );

    let space = plain_press(KeyCode::Char(' '));
    assert_eq!(
        registry.lookup(&space, ActionContext::FileManager, mac),
        Some(Action::Preview)
    );
}

// ---------------------------------------------------------------------------
// 3. macOS Modifier Mapping (Command ⌘)
// ---------------------------------------------------------------------------
#[test]
fn test_3_macos_modifier_mapping() {
    let mac = Platform::Mac;
    let mode = Mode::Normal;

    // ⌘C -> Copy
    assert_eq!(
        map_key_with_platform(press(KeyCode::Char('c'), KeyModifiers::SUPER), mode, mac),
        Some(Action::Copy)
    );
    // ⌘X -> Cut
    assert_eq!(
        map_key_with_platform(press(KeyCode::Char('x'), KeyModifiers::SUPER), mode, mac),
        Some(Action::Cut)
    );
    // ⌘V -> Paste
    assert_eq!(
        map_key_with_platform(press(KeyCode::Char('v'), KeyModifiers::SUPER), mode, mac),
        Some(Action::Paste)
    );
    // ⌘A -> Select All
    assert_eq!(
        map_key_with_platform(press(KeyCode::Char('a'), KeyModifiers::SUPER), mode, mac),
        Some(Action::SelectAll)
    );
    // ⌘F -> Search
    assert_eq!(
        map_key_with_platform(press(KeyCode::Char('f'), KeyModifiers::SUPER), mode, mac),
        Some(Action::StartSearch)
    );
    // ⌘R -> Refresh Directory
    assert_eq!(
        map_key_with_platform(press(KeyCode::Char('r'), KeyModifiers::SUPER), mode, mac),
        Some(Action::RefreshDirectory)
    );
    // ⌘K -> Command Palette
    assert_eq!(
        map_key_with_platform(press(KeyCode::Char('k'), KeyModifiers::SUPER), mode, mac),
        Some(Action::CommandPalette)
    );
    // ⌘L -> Jump to Path
    assert_eq!(
        map_key_with_platform(press(KeyCode::Char('l'), KeyModifiers::SUPER), mode, mac),
        Some(Action::JumpToPath)
    );
    // ⌘Shift+N -> New Directory
    assert_eq!(
        map_key_with_platform(
            press(
                KeyCode::Char('n'),
                KeyModifiers::SUPER | KeyModifiers::SHIFT
            ),
            mode,
            mac
        ),
        Some(Action::NewDirectory)
    );
}

// ---------------------------------------------------------------------------
// 4. Windows / Linux Modifier Mapping (Control Ctrl)
// ---------------------------------------------------------------------------
#[test]
fn test_4_windows_linux_modifier_mapping() {
    let platforms = [Platform::Windows, Platform::Linux];
    let mode = Mode::Normal;

    for platform in platforms {
        // Ctrl+C -> Copy
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('c'), KeyModifiers::CONTROL),
                mode,
                platform
            ),
            Some(Action::Copy)
        );
        // Ctrl+X -> Cut
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('x'), KeyModifiers::CONTROL),
                mode,
                platform
            ),
            Some(Action::Cut)
        );
        // Ctrl+V -> Paste
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('v'), KeyModifiers::CONTROL),
                mode,
                platform
            ),
            Some(Action::Paste)
        );
        // Ctrl+A -> Select All
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('a'), KeyModifiers::CONTROL),
                mode,
                platform
            ),
            Some(Action::SelectAll)
        );
        // Ctrl+F -> Search
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('f'), KeyModifiers::CONTROL),
                mode,
                platform
            ),
            Some(Action::StartSearch)
        );
        // Ctrl+R -> Refresh Directory
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('r'), KeyModifiers::CONTROL),
                mode,
                platform
            ),
            Some(Action::RefreshDirectory)
        );
        // Ctrl+K -> Command Palette
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('k'), KeyModifiers::CONTROL),
                mode,
                platform
            ),
            Some(Action::CommandPalette)
        );
        // Ctrl+L -> Jump to Path
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('l'), KeyModifiers::CONTROL),
                mode,
                platform
            ),
            Some(Action::JumpToPath)
        );
        // Ctrl+Shift+N -> New Directory
        assert_eq!(
            map_key_with_platform(
                press(
                    KeyCode::Char('n'),
                    KeyModifiers::CONTROL | KeyModifiers::SHIFT
                ),
                mode,
                platform
            ),
            Some(Action::NewDirectory)
        );
    }
}

// ---------------------------------------------------------------------------
// 5. Context Filtering
// ---------------------------------------------------------------------------
#[test]
fn test_5_context_filtering() {
    let mac = Platform::Mac;

    // F2 is Rename in FileManager, but NOT in Search or Terminal
    let f2 = plain_press(KeyCode::F(2));
    assert_eq!(
        map_key_with_platform(f2, Mode::Normal, mac),
        Some(Action::Rename)
    );
    assert_eq!(map_key_with_platform(f2, Mode::Search, mac), None);
    assert_eq!(map_key_with_platform(f2, Mode::Terminal, mac), None);

    // Tab is SwitchPane in FileManager, but CycleSearchMode in Search
    let tab = plain_press(KeyCode::Tab);
    assert_eq!(
        map_key_with_platform(tab, Mode::Normal, mac),
        Some(Action::SwitchPane)
    );
    assert_eq!(
        map_key_with_platform(tab, Mode::Search, mac),
        Some(Action::CycleSearchMode)
    );

    // Terminal mode sends raw keys as TerminalKey
    let char_a = plain_press(KeyCode::Char('a'));
    assert_eq!(
        map_key_event_with_platform(char_a, Mode::Terminal, mac),
        InputEvent::TerminalKey(char_a)
    );
}

// ---------------------------------------------------------------------------
// 6. Conflict Detection
// ---------------------------------------------------------------------------
#[test]
fn test_6_conflict_detection() {
    let default_reg = ShortcutRegistry::global();
    let conflicts = default_reg.detect_conflicts();
    assert!(
        conflicts.is_empty(),
        "Default shortcut registry must not contain conflicting bindings: {conflicts:#?}"
    );

    // Test artificial conflict detection
    let mut test_reg = ShortcutRegistry::default();
    test_reg.register(ShortcutBinding::primary(
        Action::Quit,
        KeyChord::plain(KeyCode::Enter),
        ActionContext::FileManager,
    ));

    let detected = test_reg.detect_conflicts();
    assert!(
        !detected.is_empty(),
        "Conflict detector must detect duplicate binding for Enter"
    );
    assert_eq!(detected[0].chord.code, KeyCode::Enter);
}

// ---------------------------------------------------------------------------
// 7. Shortcut Formatting
// ---------------------------------------------------------------------------
#[test]
fn test_7_shortcut_formatting() {
    let reg = ShortcutRegistry::global();

    // Copy formatting
    assert_eq!(
        reg.primary_shortcut(Action::Copy, Platform::Mac),
        Some("⌘C".to_string())
    );
    assert_eq!(
        reg.primary_shortcut(Action::Copy, Platform::Windows),
        Some("Ctrl+C".to_string())
    );
    assert_eq!(
        reg.primary_shortcut(Action::Copy, Platform::Linux),
        Some("Ctrl+C".to_string())
    );

    // Refresh formatting
    assert_eq!(
        reg.primary_shortcut(Action::RefreshDirectory, Platform::Mac),
        Some("⌘R".to_string())
    );
    assert_eq!(
        reg.primary_shortcut(Action::RefreshDirectory, Platform::Windows),
        Some("Ctrl+R".to_string())
    );

    // Alt+Left formatting
    assert_eq!(
        reg.primary_shortcut(Action::GoBack, Platform::Mac),
        Some("⌥Left".to_string())
    );
    assert_eq!(
        reg.primary_shortcut(Action::GoBack, Platform::Windows),
        Some("Alt+Left".to_string())
    );

    // NewDirectory formatting
    assert_eq!(
        reg.primary_shortcut(Action::NewDirectory, Platform::Mac),
        Some("⌘⇧N".to_string())
    );
    assert_eq!(
        reg.primary_shortcut(Action::NewDirectory, Platform::Windows),
        Some("Ctrl+Shift+N".to_string())
    );
}

// ---------------------------------------------------------------------------
// 8. Show Shortcuts Generation
// ---------------------------------------------------------------------------
#[test]
fn test_8_show_shortcuts_generation() {
    let reg = ShortcutRegistry::global();

    let mac_files = reg.shortcuts_by_category(ActionCategory::Files, Platform::Mac);
    assert!(!mac_files.is_empty());
    assert!(mac_files.iter().any(|(_, sc, _)| sc == "⌘C"));
    assert!(mac_files.iter().any(|(_, sc, _)| sc == "⌘X"));
    assert!(mac_files.iter().any(|(_, sc, _)| sc == "⌘V"));
    assert!(mac_files.iter().any(|(_, sc, _)| sc == "F2"));

    let win_files = reg.shortcuts_by_category(ActionCategory::Files, Platform::Windows);
    assert!(!win_files.is_empty());
    assert!(win_files.iter().any(|(_, sc, _)| sc == "Ctrl+C"));
    assert!(win_files.iter().any(|(_, sc, _)| sc == "Ctrl+X"));
    assert!(win_files.iter().any(|(_, sc, _)| sc == "Ctrl+V"));
}

// ---------------------------------------------------------------------------
// 9. Action Dispatch
// ---------------------------------------------------------------------------
#[test]
fn test_9_action_dispatch() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    fs::write(root.join("file1.txt"), "hello").unwrap();
    fs::write(root.join("file2.txt"), "world").unwrap();

    let mut app = App::at(root.to_path_buf()).unwrap();
    assert_eq!(app.pane(app.active_pane()).visible_count(), 2);

    // Dispatch SelectAll
    app.handle_action(Action::SelectAll);
    assert_eq!(app.pane(app.active_pane()).selected_count(), 2);

    // Dispatch DeselectAll
    app.handle_action(Action::DeselectAll);
    assert_eq!(app.pane(app.active_pane()).selected_count(), 0);

    // Dispatch Copy
    app.handle_action(Action::Copy);
    assert!(!app.clipboard().is_empty());

    // Dispatch ToggleHidden
    let initial_hidden = app.pane(app.active_pane()).show_hidden();
    app.handle_action(Action::ToggleHidden);
    assert_ne!(app.pane(app.active_pane()).show_hidden(), initial_hidden);

    // Dispatch SwitchPane
    let initial_pane = app.active_pane();
    app.handle_action(Action::SwitchPane);
    assert_ne!(app.active_pane(), initial_pane);
}

// ---------------------------------------------------------------------------
// 10. Invalid / Unavailable Action Handling
// ---------------------------------------------------------------------------
#[test]
fn test_10_invalid_and_unavailable_action_handling() {
    let action_reg = ActionRegistry::global();

    // ScrollTerminalUp is only available in Terminal context
    assert!(!action_reg.is_available(Action::ScrollTerminalUp, ActionContext::FileManager));
    assert!(action_reg.is_available(Action::ScrollTerminalUp, ActionContext::Terminal));

    // Open is available in FileManager, not in Terminal
    assert!(action_reg.is_available(Action::Open, ActionContext::FileManager));
    assert!(!action_reg.is_available(Action::Open, ActionContext::Terminal));

    // Release events are always ignored
    let release_event = KeyEvent::new_with_kind(
        KeyCode::Char('c'),
        KeyModifiers::SUPER,
        KeyEventKind::Release,
    );
    assert_eq!(
        map_key_event_with_platform(release_event, Mode::Normal, Platform::Mac),
        InputEvent::Ignored
    );
}
