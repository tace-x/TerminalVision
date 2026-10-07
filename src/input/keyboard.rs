//! Keyboard input mapping to application actions.
//!
//! Translates terminal key events into semantic [`Action`]s based on the active
//! interaction [`Mode`] using the centralized [`ShortcutRegistry`].
//!
//! This module never performs filesystem operations or state mutations directly.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::actions::Action;
use crate::app::modes::Mode;
use crate::input::InputEvent;
use crate::input::platform::Platform;
use crate::input::shortcut::ShortcutRegistry;

/// Whether a key event is an explicit interrupt request that asks the application to quit.
pub fn is_quit_request(key: KeyEvent) -> bool {
    is_quit_request_with_platform(key, Platform::current())
}

/// Whether a key event is an explicit interrupt request that asks the application to quit on `platform`.
pub fn is_quit_request_with_platform(key: KeyEvent, platform: Platform) -> bool {
    if key.kind == KeyEventKind::Release {
        return false;
    }

    let is_c = key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C');
    let is_q = key.code == KeyCode::Char('q') || key.code == KeyCode::Char('Q');

    let is_ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL) && is_c;
    let is_primary_q = if platform.is_mac() {
        key.modifiers.contains(KeyModifiers::SUPER) && is_q
    } else {
        key.modifiers.contains(KeyModifiers::CONTROL) && is_q
    };

    is_ctrl_c || is_primary_q
}

/// Maps a physical key event to an [`Action`] based on the current [`Mode`] and active [`Platform`].
pub fn map_key(key: KeyEvent, mode: Mode) -> Option<Action> {
    map_key_with_platform(key, mode, Platform::current())
}

/// Maps a key event with an explicit [`Platform`] (useful for testing cross-platform behaviors).
pub fn map_key_with_platform(key: KeyEvent, mode: Mode, platform: Platform) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }

    // In Mode::Boot, any key interrupts startup and transitions immediately to the ready UI.
    if mode == Mode::Boot {
        if is_quit_request_with_platform(key, platform) {
            return Some(Action::Quit);
        }
        return Some(Action::Cancel);
    }

    // In Terminal mode, only global terminal focus toggle is intercepted.
    // All other keys must be passed directly to the interactive PTY shell.
    if mode == Mode::Terminal {
        let registry = ShortcutRegistry::global();
        if let Some(Action::ToggleTerminalFocus) =
            registry.lookup(&key, mode.action_context(), platform)
        {
            return Some(Action::ToggleTerminalFocus);
        }
        return None;
    }

    // For non-terminal modes, look up the key in the centralized Shortcut Registry.
    let registry = ShortcutRegistry::global();
    if let Some(action) = registry.lookup(&key, mode.action_context(), platform) {
        return Some(action);
    }

    // Secondary fallback for macOS Ctrl+C to quit if not already handled
    if platform.is_mac()
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C'))
    {
        return Some(Action::Quit);
    }

    None
}

/// Translates a [`KeyEvent`] into an [`InputEvent`] taking into account mode state machine context.
pub fn map_key_event(key: KeyEvent, mode: Mode) -> InputEvent {
    map_key_event_with_platform(key, mode, Platform::current())
}

/// Translates a [`KeyEvent`] into an [`InputEvent`] for a specific [`Platform`].
pub fn map_key_event_with_platform(key: KeyEvent, mode: Mode, platform: Platform) -> InputEvent {
    if key.kind == KeyEventKind::Release {
        return InputEvent::Ignored;
    }

    // 1. Try mapping to an Action
    if let Some(action) = map_key_with_platform(key, mode, platform) {
        return InputEvent::Action(action);
    }

    // 2. In Terminal mode, all unmapped keys go straight to the PTY shell
    if mode == Mode::Terminal {
        return InputEvent::TerminalKey(key);
    }

    let has_ctrl_or_alt =
        key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT);
    let has_super = key.modifiers.contains(KeyModifiers::SUPER);

    if has_super {
        return InputEvent::Ignored;
    }

    // 3. Mode-specific text input and interaction routing
    match mode {
        Mode::Search if !has_ctrl_or_alt => match key.code {
            KeyCode::Enter => InputEvent::SearchConfirm,
            KeyCode::Backspace => InputEvent::SearchBackspace,
            KeyCode::Up => InputEvent::Action(Action::MoveUp),
            KeyCode::Down => InputEvent::Action(Action::MoveDown),
            KeyCode::Char(c) => InputEvent::SearchChar(c),
            _ => InputEvent::Ignored,
        },
        Mode::Create | Mode::Rename | Mode::Jump if !has_ctrl_or_alt => match key.code {
            KeyCode::Enter => InputEvent::ModalConfirm,
            KeyCode::Backspace => InputEvent::ModalBackspace,
            KeyCode::Delete => InputEvent::ModalDelete,
            KeyCode::Left => InputEvent::ModalMoveCursorLeft,
            KeyCode::Right => InputEvent::ModalMoveCursorRight,
            KeyCode::Home => InputEvent::ModalMoveCursorHome,
            KeyCode::End => InputEvent::ModalMoveCursorEnd,
            KeyCode::Char(c) => InputEvent::ModalChar(c),
            _ => InputEvent::Ignored,
        },
        Mode::Confirm if !has_ctrl_or_alt => match key.code {
            KeyCode::Enter => InputEvent::ModalConfirm,
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Char('h')
            | KeyCode::Char('l') => InputEvent::ModalToggle,
            KeyCode::Char('y') | KeyCode::Char('Y') => InputEvent::ModalSetConfirm(true),
            KeyCode::Char('n') | KeyCode::Char('N') => InputEvent::ModalSetConfirm(false),
            _ => InputEvent::Ignored,
        },
        Mode::CommandPalette | Mode::SmartJump if !has_ctrl_or_alt => match key.code {
            KeyCode::Enter => InputEvent::ModalConfirm,
            KeyCode::Backspace => InputEvent::ModalBackspace,
            KeyCode::Up => InputEvent::ModalNavigateUp,
            KeyCode::Down => InputEvent::ModalNavigateDown,
            KeyCode::Char(c) => InputEvent::ModalChar(c),
            _ => InputEvent::Ignored,
        },
        Mode::Bookmarks if !has_ctrl_or_alt => match key.code {
            KeyCode::Enter => InputEvent::ModalConfirm,
            KeyCode::Up | KeyCode::Char('k') => InputEvent::ModalNavigateUp,
            KeyCode::Down | KeyCode::Char('j') => InputEvent::ModalNavigateDown,
            KeyCode::Char('K') | KeyCode::Char('u') => InputEvent::Action(Action::MoveFavoriteUp),
            KeyCode::Char('J') | KeyCode::Char('m') => InputEvent::Action(Action::MoveFavoriteDown),
            KeyCode::Char('d') | KeyCode::Char('x') | KeyCode::Delete => {
                InputEvent::Action(Action::RemoveBookmark)
            }
            KeyCode::Char('q') | KeyCode::Esc => InputEvent::Action(Action::Cancel),
            _ => InputEvent::Ignored,
        },
        Mode::StorageVision if !has_ctrl_or_alt => match key.code {
            KeyCode::Enter => InputEvent::ModalConfirm,
            KeyCode::Up | KeyCode::Char('k') => InputEvent::ModalNavigateUp,
            KeyCode::Down | KeyCode::Char('j') => InputEvent::ModalNavigateDown,
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                InputEvent::Action(Action::GoParent)
            }
            KeyCode::Char('o') | KeyCode::Char('O') => InputEvent::Action(Action::Open),
            KeyCode::Char('q') | KeyCode::Esc => InputEvent::Action(Action::Cancel),
            _ => InputEvent::Ignored,
        },
        Mode::ThemeSelector if !has_ctrl_or_alt => match key.code {
            KeyCode::Enter => InputEvent::ModalConfirm,
            KeyCode::Up | KeyCode::Char('k') => InputEvent::ModalNavigateUp,
            KeyCode::Down | KeyCode::Char('j') => InputEvent::ModalNavigateDown,
            KeyCode::Char('q') | KeyCode::Esc => InputEvent::Action(Action::Cancel),
            _ => InputEvent::Ignored,
        },
        Mode::ProjectCockpit | Mode::GitStatusPanel | Mode::RevealContext if !has_ctrl_or_alt => {
            match key.code {
                KeyCode::Enter => InputEvent::ModalConfirm,
                KeyCode::Up | KeyCode::Char('k') => InputEvent::ModalNavigateUp,
                KeyCode::Down | KeyCode::Char('j') => InputEvent::ModalNavigateDown,
                KeyCode::Char('q') => InputEvent::Action(Action::Cancel),
                _ => InputEvent::Ignored,
            }
        }
        Mode::FileRadar if !has_ctrl_or_alt => match key.code {
            KeyCode::Enter | KeyCode::Char('q') => InputEvent::Action(Action::Cancel),
            _ => InputEvent::Ignored,
        },
        Mode::ContextMenu if !has_ctrl_or_alt => match key.code {
            KeyCode::Enter => InputEvent::ModalConfirm,
            KeyCode::Up | KeyCode::Char('k') => InputEvent::ModalNavigateUp,
            KeyCode::Down | KeyCode::Char('j') => InputEvent::ModalNavigateDown,
            KeyCode::Right | KeyCode::Char('l') => InputEvent::ModalNavigateRight,
            KeyCode::Left | KeyCode::Char('h') => InputEvent::ModalNavigateLeft,
            KeyCode::Home => InputEvent::ModalMoveCursorHome,
            KeyCode::End => InputEvent::ModalMoveCursorEnd,
            KeyCode::PageUp => InputEvent::ModalPageUp,
            KeyCode::PageDown => InputEvent::ModalPageDown,
            KeyCode::Esc => InputEvent::Action(Action::Cancel),
            KeyCode::Char(c) => InputEvent::ModalChar(c),
            _ => InputEvent::Ignored,
        },
        Mode::Help | Mode::Preview if !has_ctrl_or_alt => match key.code {
            KeyCode::Enter => InputEvent::Action(Action::Cancel),
            _ => InputEvent::Ignored,
        },
        _ => InputEvent::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn plain_press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_quit_requests() {
        assert!(is_quit_request(press(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
        assert!(is_quit_request(press(
            KeyCode::Char('C'),
            KeyModifiers::CONTROL
        )));

        // Test across platforms
        assert!(is_quit_request_with_platform(
            press(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Platform::Mac
        ));
        assert!(is_quit_request_with_platform(
            press(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Platform::Linux
        ));
        assert!(is_quit_request_with_platform(
            press(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Platform::Windows
        ));

        assert!(is_quit_request_with_platform(
            press(KeyCode::Char('q'), KeyModifiers::SUPER),
            Platform::Mac
        ));
        assert!(is_quit_request_with_platform(
            press(KeyCode::Char('q'), KeyModifiers::CONTROL),
            Platform::Linux
        ));
        assert!(is_quit_request_with_platform(
            press(KeyCode::Char('q'), KeyModifiers::CONTROL),
            Platform::Windows
        ));
    }

    #[test]
    fn test_normal_mode_navigation_mappings() {
        assert_eq!(
            map_key(plain_press(KeyCode::Up), Mode::Normal),
            Some(Action::MoveUp)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('k')), Mode::Normal),
            Some(Action::MoveUp)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Down), Mode::Normal),
            Some(Action::MoveDown)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('j')), Mode::Normal),
            Some(Action::MoveDown)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Left), Mode::Normal),
            Some(Action::MoveLeft)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('h')), Mode::Normal),
            Some(Action::MoveLeft)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Right), Mode::Normal),
            Some(Action::MoveRight)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('l')), Mode::Normal),
            Some(Action::MoveRight)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Enter), Mode::Normal),
            Some(Action::Open)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Backspace), Mode::Normal),
            Some(Action::GoParent)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Home), Mode::Normal),
            Some(Action::GoHome)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::End), Mode::Normal),
            Some(Action::GoEnd)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::PageUp), Mode::Normal),
            Some(Action::PageUp)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::PageDown), Mode::Normal),
            Some(Action::PageDown)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Tab), Mode::Normal),
            Some(Action::SwitchPane)
        );
    }

    #[test]
    fn test_normal_mode_file_operations_mappings() {
        assert_eq!(
            map_key(plain_press(KeyCode::Char('n')), Mode::Normal),
            Some(Action::NewFile)
        );
        assert_eq!(
            map_key(press(KeyCode::Char('N'), KeyModifiers::SHIFT), Mode::Normal),
            Some(Action::NewDirectory)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('N')), Mode::Normal),
            Some(Action::NewDirectory)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('r')), Mode::Normal),
            Some(Action::Rename)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::F(2)), Mode::Normal),
            Some(Action::Rename)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('y')), Mode::Normal),
            Some(Action::Copy)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('x')), Mode::Normal),
            Some(Action::Cut)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('p')), Mode::Normal),
            Some(Action::Paste)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('d')), Mode::Normal),
            Some(Action::Delete)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Delete), Mode::Normal),
            Some(Action::Delete)
        );
    }

    #[test]
    fn test_cross_platform_shortcuts() {
        let mac = Platform::Mac;
        let win = Platform::Windows;

        // Copy
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('c'), KeyModifiers::SUPER),
                Mode::Normal,
                mac
            ),
            Some(Action::Copy)
        );
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('c'), KeyModifiers::CONTROL),
                Mode::Normal,
                win
            ),
            Some(Action::Copy)
        );

        // Select All
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('a'), KeyModifiers::SUPER),
                Mode::Normal,
                mac
            ),
            Some(Action::SelectAll)
        );
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('a'), KeyModifiers::CONTROL),
                Mode::Normal,
                win
            ),
            Some(Action::SelectAll)
        );

        // Refresh
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('r'), KeyModifiers::SUPER),
                Mode::Normal,
                mac
            ),
            Some(Action::RefreshDirectory)
        );
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('r'), KeyModifiers::CONTROL),
                Mode::Normal,
                win
            ),
            Some(Action::RefreshDirectory)
        );

        // Command Palette / Command Center
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('k'), KeyModifiers::SUPER),
                Mode::Normal,
                mac
            ),
            Some(Action::CommandPalette)
        );
        assert_eq!(
            map_key_with_platform(
                press(KeyCode::Char('k'), KeyModifiers::CONTROL),
                Mode::Normal,
                win
            ),
            Some(Action::CommandPalette)
        );
    }

    #[test]
    fn test_terminal_focus_safety() {
        let mac = Platform::Mac;

        // Terminal mode forwards normal keys as TerminalKey
        let enter = plain_press(KeyCode::Enter);
        assert_eq!(
            map_key_event_with_platform(enter, Mode::Terminal, mac),
            InputEvent::TerminalKey(enter)
        );

        let ctrl_c = press(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(
            map_key_event_with_platform(ctrl_c, Mode::Terminal, mac),
            InputEvent::TerminalKey(ctrl_c)
        );

        // Terminal focus toggle intercepts in Terminal mode
        let ctrl_t = press(KeyCode::Char('t'), KeyModifiers::CONTROL);
        assert_eq!(
            map_key_event_with_platform(ctrl_t, Mode::Terminal, mac),
            InputEvent::Action(Action::ToggleTerminalFocus)
        );

        let f12 = plain_press(KeyCode::F(12));
        assert_eq!(
            map_key_event_with_platform(f12, Mode::Terminal, mac),
            InputEvent::Action(Action::ToggleTerminalFocus)
        );
    }

    #[test]
    fn test_search_mode_event_mapping() {
        assert_eq!(
            map_key_event(plain_press(KeyCode::Esc), Mode::Search),
            InputEvent::Action(Action::Cancel)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Enter), Mode::Search),
            InputEvent::SearchConfirm
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('a')), Mode::Search),
            InputEvent::SearchChar('a')
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('Z')), Mode::Search),
            InputEvent::SearchChar('Z')
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Backspace), Mode::Search),
            InputEvent::SearchBackspace
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Tab), Mode::Search),
            InputEvent::Action(Action::CycleSearchMode)
        );
    }

    #[test]
    fn test_key_release_is_ignored() {
        let release = KeyEvent::new_with_kind(
            KeyCode::Char('k'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );
        assert_eq!(map_key(release, Mode::Normal), None);
        assert_eq!(map_key_event(release, Mode::Normal), InputEvent::Ignored);
    }
}
