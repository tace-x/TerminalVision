//! The input layer.
//!
//! Terminal events are read here and classified into something the application
//! loop can act on: an action, search typing, a resize, or an ignored event.
//! Classifying is all it does: the layer never changes application state, never
//! touches the filesystem, and never decides how anything is drawn.

pub mod keyboard;
pub mod mouse;
pub mod platform;
pub mod shortcut;

pub use platform::Platform;
pub use shortcut::{
    KeyChord, Modifiers, ShortcutBinding, ShortcutConflict, ShortcutFormatter, ShortcutRegistry,
};

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event};

use crate::app::actions::Action;
use crate::app::modes::Mode;
use crate::layout::geometry::TerminalSize;

/// What the input layer made of a terminal event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// An action the event asked for.
    Action(Action),
    /// A character typed into the active search query.
    SearchChar(char),
    /// Backspace pressed while typing a search query.
    SearchBackspace,
    /// Enter pressed to confirm search query.
    SearchConfirm,
    /// A character typed into a modal input field or command palette.
    ModalChar(char),
    /// Backspace pressed in a modal input field or command palette.
    ModalBackspace,
    /// Enter pressed to confirm a modal action.
    ModalConfirm,
    /// Move cursor left in an input dialog.
    ModalMoveCursorLeft,
    /// Move cursor right in an input dialog.
    ModalMoveCursorRight,
    /// Move cursor to the start of the input field.
    ModalMoveCursorHome,
    /// Move cursor to the end of the input field.
    ModalMoveCursorEnd,
    /// Delete character at cursor in an input dialog.
    ModalDelete,
    /// Navigate up in a modal list (e.g. command palette).
    ModalNavigateUp,
    /// Navigate down in a modal list (e.g. command palette).
    ModalNavigateDown,
    /// Navigate left / collapse submenu in a modal.
    ModalNavigateLeft,
    /// Navigate right / expand submenu in a modal.
    ModalNavigateRight,
    /// Toggle selected option in a confirmation dialog.
    ModalToggle,
    /// Explicitly choose Yes or No in a confirmation dialog.
    ModalSetConfirm(bool),
    /// A key event directed to the interactive terminal shell.
    TerminalKey(crossterm::event::KeyEvent),
    /// A mouse event reported by the terminal.
    Mouse(crossterm::event::MouseEvent),
    /// The terminal was resized, carrying the size the terminal reported.
    Resize(TerminalSize),
    /// An event nothing acts on.
    Ignored,
}

/// Waits up to `timeout` for a terminal event, and classifies it based on `mode`.
///
/// Returns `Ok(None)` when the timeout passes without an event, so a caller can
/// do other work between events instead of blocking forever.
pub fn next_event(timeout: Duration, mode: Mode) -> io::Result<Option<InputEvent>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }

    Ok(Some(match event::read()? {
        Event::Resize(columns, rows) => InputEvent::Resize(TerminalSize::new(columns, rows)),
        Event::Key(key) => keyboard::map_key_event(key, mode),
        Event::Mouse(mouse) => InputEvent::Mouse(mouse),
        _ => InputEvent::Ignored,
    }))
}

#[cfg(test)]
mod tests {
    use super::InputEvent;
    use crate::app::actions::Action;
    use crate::layout::geometry::TerminalSize;

    #[test]
    fn an_input_event_carries_existing_types() {
        let action = InputEvent::Action(Action::MoveDown);
        let resize = InputEvent::Resize(TerminalSize::new(80, 24));
        let search_ch = InputEvent::SearchChar('a');
        let search_bk = InputEvent::SearchBackspace;
        let ignored = InputEvent::Ignored;

        assert_ne!(action, resize);
        assert_ne!(resize, ignored);
        assert_ne!(ignored, action);
        assert_ne!(search_ch, search_bk);
    }
}
