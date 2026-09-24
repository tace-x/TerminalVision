//! Keyboard input mapping to application actions.
//!
//! Translates terminal key events into semantic [`Action`]s based on the active
//! interaction [`Mode`].
//!
//! This module never performs filesystem operations or state mutations directly.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::actions::Action;
use crate::app::modes::Mode;
use crate::input::InputEvent;

/// Whether a key event is the interrupt (Ctrl+C) that asks the application to stop.
pub fn is_quit_request(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C'))
}

/// Maps a physical key event to an [`Action`] based on the current [`Mode`].
pub fn map_key(key: KeyEvent, mode: Mode) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }

    // Ctrl+C is an unconditional quit request across all modes.
    if is_quit_request(key) {
        return Some(Action::Quit);
    }

    // Ctrl+P opens the command palette.
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && (key.code == KeyCode::Char('p') || key.code == KeyCode::Char('P'))
    {
        return Some(Action::CommandPalette);
    }

    // Ctrl+A in Normal mode selects all entries.
    if mode == Mode::Normal
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && (key.code == KeyCode::Char('a') || key.code == KeyCode::Char('A'))
    {
        return Some(Action::SelectAll);
    }

    if mode == Mode::Normal
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && (key.code == KeyCode::Char('f') || key.code == KeyCode::Char('F'))
    {
        return Some(Action::ToggleFocusMode);
    }

    // If other Ctrl or Alt modifiers are active, do not trigger normal single-key actions.
    // Alt+Left/Right navigate tab history regardless of active mode.
    if key.modifiers.contains(KeyModifiers::ALT) {
        if key.code == KeyCode::Left {
            return Some(Action::GoBack);
        }
        if key.code == KeyCode::Right {
            return Some(Action::GoForward);
        }
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }

    match mode {
        Mode::Normal => map_normal_key(key),
        Mode::Search => map_search_action(key),
        Mode::Preview | Mode::Help => map_overlay_key(key),
        Mode::Rename
        | Mode::Create
        | Mode::Confirm
        | Mode::CommandPalette
        | Mode::Bookmarks
        | Mode::Jump
        | Mode::SmartJump
        | Mode::ProjectCockpit
        | Mode::GitStatusPanel
        | Mode::FileRadar
        | Mode::RevealContext => map_input_modal_key(key),
    }
}

/// Maps a key event in Normal mode.
fn map_normal_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        // Navigation
        KeyCode::Up | KeyCode::Char('k') => Some(Action::MoveUp),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::MoveDown),
        KeyCode::Left | KeyCode::Char('h') => Some(Action::MoveLeft),
        KeyCode::Right | KeyCode::Char('l') => Some(Action::MoveRight),
        KeyCode::Enter => Some(Action::Open),
        KeyCode::Backspace => Some(Action::GoParent),
        KeyCode::Home => Some(Action::GoHome),
        KeyCode::End => Some(Action::GoEnd),
        KeyCode::PageUp => Some(Action::PageUp),
        KeyCode::PageDown => Some(Action::PageDown),
        KeyCode::Tab => Some(Action::SwitchPane),

        // Path & Smart jump
        KeyCode::Char('g') => Some(Action::JumpToPath),
        KeyCode::Char('J') => Some(Action::SmartJump),

        // Developer Intelligence & Power Tools
        KeyCode::Char('P') => Some(Action::ProjectCockpit),
        KeyCode::Char('G') => Some(Action::GitStatusPanel),
        KeyCode::Char('F') => Some(Action::FileRadar),
        KeyCode::Char('C') => Some(Action::RevealContext),

        // File operations
        KeyCode::Char('n') => Some(Action::NewFile),
        KeyCode::Char('N') => Some(Action::NewDirectory),
        KeyCode::Char('r') => Some(Action::Rename),
        KeyCode::Char('y') => Some(Action::Copy),
        KeyCode::Char('x') => Some(Action::Cut),
        KeyCode::Char('p') => Some(Action::Paste),
        KeyCode::Char('d') => Some(Action::Delete),

        // Multi-selection
        KeyCode::Char(' ') => Some(Action::ToggleSelect),
        KeyCode::Char('*') => Some(Action::InvertSelection),
        KeyCode::Char('u') => Some(Action::DeselectAll),

        // View
        KeyCode::Char('.') => Some(Action::ToggleHidden),
        KeyCode::Char('s') => Some(Action::ChangeSort),
        KeyCode::Char('v') => Some(Action::Preview),
        KeyCode::Char('z') | KeyCode::Char('Z') => Some(Action::ToggleFocusMode),

        // Bookmarks
        KeyCode::Char('b') => Some(Action::AddBookmark),
        KeyCode::Char('B') => Some(Action::OpenBookmarks),

        // Tabs
        KeyCode::Char('t') => Some(Action::NewTab),
        KeyCode::Char('T') => Some(Action::DuplicateTab),
        KeyCode::Char('w') => Some(Action::CloseTab),
        KeyCode::Char(']') => Some(Action::NextTab),
        KeyCode::Char('[') => Some(Action::PreviousTab),

        // Search
        KeyCode::Char('/') => Some(Action::StartSearch),

        // Application
        KeyCode::Char('?') => Some(Action::Help),
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Esc => Some(Action::Cancel),

        _ => None,
    }
}

/// Maps actions in Search mode (Esc cancels, Tab cycles mode).
fn map_search_action(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::Cancel),
        KeyCode::Tab => Some(Action::CycleSearchMode),
        _ => None,
    }
}

/// Maps keys in viewer overlay modes (Preview and Help).
fn map_overlay_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::Cancel),
        _ => None,
    }
}

/// Maps keys in text input / confirmation modal modes.
fn map_input_modal_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::Cancel),
        _ => None,
    }
}

/// Translates a [`KeyEvent`] into an [`InputEvent`].
pub fn map_key_event(key: KeyEvent, mode: Mode) -> InputEvent {
    if key.kind == KeyEventKind::Release {
        return InputEvent::Ignored;
    }

    if let Some(action) = map_key(key, mode) {
        return InputEvent::Action(action);
    }

    let has_ctrl_or_alt =
        key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT);

    match mode {
        Mode::Search if !has_ctrl_or_alt => match key.code {
            KeyCode::Enter => InputEvent::SearchConfirm,
            KeyCode::Backspace => InputEvent::SearchBackspace,
            KeyCode::Char(c) => InputEvent::SearchChar(c),
            // Tab is handled as an Action (CycleSearchMode) above via map_key.
            _ => InputEvent::Ignored,
        },
        Mode::Create | Mode::Rename | Mode::Jump if !has_ctrl_or_alt => match key.code {
            KeyCode::Enter => InputEvent::ModalConfirm,
            KeyCode::Backspace => InputEvent::ModalBackspace,
            KeyCode::Left => InputEvent::ModalMoveCursorLeft,
            KeyCode::Right => InputEvent::ModalMoveCursorRight,
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
            KeyCode::Char('d') | KeyCode::Char('x') | KeyCode::Delete => {
                InputEvent::Action(Action::RemoveBookmark)
            }
            KeyCode::Char('q') => InputEvent::Action(Action::Cancel),
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
        assert!(!is_quit_request(plain_press(KeyCode::Char('c'))));
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
    }

    #[test]
    fn test_normal_mode_view_mappings() {
        assert_eq!(
            map_key(plain_press(KeyCode::Char('.')), Mode::Normal),
            Some(Action::ToggleHidden)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('s')), Mode::Normal),
            Some(Action::ChangeSort)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('v')), Mode::Normal),
            Some(Action::Preview)
        );
    }

    #[test]
    fn test_normal_mode_search_and_app_mappings() {
        assert_eq!(
            map_key(plain_press(KeyCode::Char('/')), Mode::Normal),
            Some(Action::StartSearch)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('?')), Mode::Normal),
            Some(Action::Help)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('q')), Mode::Normal),
            Some(Action::Quit)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Esc), Mode::Normal),
            Some(Action::Cancel)
        );
        assert_eq!(
            map_key(
                press(KeyCode::Char('c'), KeyModifiers::CONTROL),
                Mode::Normal
            ),
            Some(Action::Quit)
        );
        assert_eq!(
            map_key(
                press(KeyCode::Char('p'), KeyModifiers::CONTROL),
                Mode::Normal
            ),
            Some(Action::CommandPalette)
        );
        assert_eq!(
            map_key(
                press(KeyCode::Char('P'), KeyModifiers::CONTROL),
                Mode::Normal
            ),
            Some(Action::CommandPalette)
        );
    }

    #[test]
    fn test_modifier_handling() {
        // Alt modifiers should be ignored for single-key shortcuts
        assert_eq!(
            map_key(press(KeyCode::Char('d'), KeyModifiers::ALT), Mode::Normal),
            None
        );
        assert_eq!(
            map_key(
                press(KeyCode::Char('k'), KeyModifiers::CONTROL),
                Mode::Normal
            ),
            None
        );
    }

    #[test]
    fn test_temporary_modes_do_not_trigger_normal_actions() {
        let temp_modes = [
            Mode::Search,
            Mode::Rename,
            Mode::Create,
            Mode::Confirm,
            Mode::Preview,
            Mode::CommandPalette,
            Mode::Help,
            Mode::Bookmarks,
            Mode::Jump,
            Mode::SmartJump,
        ];

        for mode in temp_modes {
            // Normal actions like delete, rename, navigation must NOT trigger in temp modes
            assert_eq!(map_key(plain_press(KeyCode::Char('d')), mode), None);
            assert_eq!(map_key(plain_press(KeyCode::Char('r')), mode), None);
            assert_eq!(map_key(plain_press(KeyCode::Char('y')), mode), None);
            assert_eq!(map_key(plain_press(KeyCode::Char('n')), mode), None);

            // Esc always produces Cancel
            assert_eq!(
                map_key(plain_press(KeyCode::Esc), mode),
                Some(Action::Cancel)
            );
            // Ctrl+C always produces Quit
            assert_eq!(
                map_key(press(KeyCode::Char('c'), KeyModifiers::CONTROL), mode),
                Some(Action::Quit)
            );
        }
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

    #[test]
    fn test_input_modal_event_mapping() {
        for mode in [Mode::Create, Mode::Rename] {
            assert_eq!(
                map_key_event(plain_press(KeyCode::Char('a')), mode),
                InputEvent::ModalChar('a')
            );
            assert_eq!(
                map_key_event(plain_press(KeyCode::Char('文')), mode),
                InputEvent::ModalChar('文')
            );
            assert_eq!(
                map_key_event(plain_press(KeyCode::Backspace), mode),
                InputEvent::ModalBackspace
            );
            assert_eq!(
                map_key_event(plain_press(KeyCode::Enter), mode),
                InputEvent::ModalConfirm
            );
            assert_eq!(
                map_key_event(plain_press(KeyCode::Left), mode),
                InputEvent::ModalMoveCursorLeft
            );
            assert_eq!(
                map_key_event(plain_press(KeyCode::Right), mode),
                InputEvent::ModalMoveCursorRight
            );
            assert_eq!(
                map_key_event(plain_press(KeyCode::Esc), mode),
                InputEvent::Action(Action::Cancel)
            );
        }
    }

    #[test]
    fn test_confirm_modal_event_mapping() {
        assert_eq!(
            map_key_event(plain_press(KeyCode::Left), Mode::Confirm),
            InputEvent::ModalToggle
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Right), Mode::Confirm),
            InputEvent::ModalToggle
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Tab), Mode::Confirm),
            InputEvent::ModalToggle
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('y')), Mode::Confirm),
            InputEvent::ModalSetConfirm(true)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('Y')), Mode::Confirm),
            InputEvent::ModalSetConfirm(true)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('n')), Mode::Confirm),
            InputEvent::ModalSetConfirm(false)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('N')), Mode::Confirm),
            InputEvent::ModalSetConfirm(false)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Enter), Mode::Confirm),
            InputEvent::ModalConfirm
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Esc), Mode::Confirm),
            InputEvent::Action(Action::Cancel)
        );
    }

    #[test]
    fn test_command_palette_event_mapping() {
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('f')), Mode::CommandPalette),
            InputEvent::ModalChar('f')
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Backspace), Mode::CommandPalette),
            InputEvent::ModalBackspace
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Up), Mode::CommandPalette),
            InputEvent::ModalNavigateUp
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Down), Mode::CommandPalette),
            InputEvent::ModalNavigateDown
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Enter), Mode::CommandPalette),
            InputEvent::ModalConfirm
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Esc), Mode::CommandPalette),
            InputEvent::Action(Action::Cancel)
        );
    }

    #[test]
    fn test_help_overlay_event_mapping() {
        assert_eq!(
            map_key_event(plain_press(KeyCode::Esc), Mode::Help),
            InputEvent::Action(Action::Cancel)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('q')), Mode::Help),
            InputEvent::Action(Action::Cancel)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Enter), Mode::Help),
            InputEvent::Action(Action::Cancel)
        );
    }

    #[test]
    fn test_bookmarks_event_mapping() {
        assert_eq!(
            map_key(plain_press(KeyCode::Char('b')), Mode::Normal),
            Some(Action::AddBookmark)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('B')), Mode::Normal),
            Some(Action::OpenBookmarks)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Up), Mode::Bookmarks),
            InputEvent::ModalNavigateUp
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('k')), Mode::Bookmarks),
            InputEvent::ModalNavigateUp
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Down), Mode::Bookmarks),
            InputEvent::ModalNavigateDown
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('j')), Mode::Bookmarks),
            InputEvent::ModalNavigateDown
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Enter), Mode::Bookmarks),
            InputEvent::ModalConfirm
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('d')), Mode::Bookmarks),
            InputEvent::Action(Action::RemoveBookmark)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('x')), Mode::Bookmarks),
            InputEvent::Action(Action::RemoveBookmark)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Delete), Mode::Bookmarks),
            InputEvent::Action(Action::RemoveBookmark)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Esc), Mode::Bookmarks),
            InputEvent::Action(Action::Cancel)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('q')), Mode::Bookmarks),
            InputEvent::Action(Action::Cancel)
        );
    }

    #[test]
    fn test_tabs_event_mapping() {
        assert_eq!(
            map_key(plain_press(KeyCode::Char('t')), Mode::Normal),
            Some(Action::NewTab)
        );
        assert_eq!(
            map_key(press(KeyCode::Char('T'), KeyModifiers::SHIFT), Mode::Normal),
            Some(Action::DuplicateTab)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('T')), Mode::Normal),
            Some(Action::DuplicateTab)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('w')), Mode::Normal),
            Some(Action::CloseTab)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char(']')), Mode::Normal),
            Some(Action::NextTab)
        );
        assert_eq!(
            map_key(plain_press(KeyCode::Char('[')), Mode::Normal),
            Some(Action::PreviousTab)
        );
    }

    #[test]
    fn test_smart_jump_event_mapping() {
        assert_eq!(
            map_key(plain_press(KeyCode::Char('J')), Mode::Normal),
            Some(Action::SmartJump)
        );
        assert_eq!(
            map_key(press(KeyCode::Char('J'), KeyModifiers::SHIFT), Mode::Normal),
            Some(Action::SmartJump)
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Char('h')), Mode::SmartJump),
            InputEvent::ModalChar('h')
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Backspace), Mode::SmartJump),
            InputEvent::ModalBackspace
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Up), Mode::SmartJump),
            InputEvent::ModalNavigateUp
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Down), Mode::SmartJump),
            InputEvent::ModalNavigateDown
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Enter), Mode::SmartJump),
            InputEvent::ModalConfirm
        );
        assert_eq!(
            map_key_event(plain_press(KeyCode::Esc), Mode::SmartJump),
            InputEvent::Action(Action::Cancel)
        );
    }
}
