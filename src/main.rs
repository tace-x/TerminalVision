//! The TerminalVision binary.
//!
//! The layers of the application live in the library; this file takes over the
//! terminal, runs the loop, and hands the terminal back.

use std::io::{self, IsTerminal, Write};
use std::panic;
use std::time::Duration;

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use terminalvision::app::state::App;
use terminalvision::input::{InputEvent, next_event};
use terminalvision::layout::geometry::TerminalSize;

/// How long the loop waits for input before checking whether to stop.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

fn main() {
    if let Err(error) = run() {
        // The terminal has already been handed back by this point, so the
        // message is readable.
        eprintln!("TerminalVision: {error}");
        std::process::exit(1);
    }
}

/// Runs the application until it asks to stop.
fn run() -> io::Result<()> {
    if !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "standard output is not a terminal, so TerminalVision cannot take it over",
        ));
    }

    let mut app = start_application()?;

    let mut session = TerminalSession::enter()?;
    install_panic_hook();

    let mut size = session.size()?;
    let mut mouse_tracker = terminalvision::input::mouse::MouseTracker::new();

    // Visibly render the initial interface before waiting for input.
    session.draw(&app)?;

    while !app.should_quit() {
        if let Some(event) = next_event(POLL_INTERVAL, app.mode())? {
            apply_input_event(&mut app, event, &mut size, &mut mouse_tracker);
            if !app.should_quit() {
                session.draw(&app)?;
            }
        }
    }

    let _ = app.save_persistent_state();

    session.leave()
}

/// The application the loop runs, started in the working directory of the
/// process.
///
/// The location is read from the operating system rather than assumed, and a
/// process whose working directory cannot be determined is reported instead of
/// being given a directory it was never started in.
fn start_application() -> io::Result<App> {
    let mut app = App::at_working_directory().map_err(io::Error::other)?;
    let _ = app.load_persistent_state();
    Ok(app)
}

/// Applies one input event.
///
/// Kept apart from the loop so that everything except obtaining the event can be
/// exercised without a terminal.
fn apply_input_event(
    app: &mut App,
    event: InputEvent,
    size: &mut TerminalSize,
    mouse_tracker: &mut terminalvision::input::mouse::MouseTracker,
) {
    match event {
        InputEvent::Action(action) => app.handle_action(action),
        InputEvent::SearchChar(ch) => {
            let mut query = app.search().query().to_string();
            query.push(ch);
            app.set_search_query(&query);
        }
        InputEvent::SearchBackspace => {
            let mut query = app.search().query().to_string();
            query.pop();
            app.set_search_query(&query);
        }
        InputEvent::SearchConfirm => {
            app.confirm_search();
        }
        InputEvent::ModalChar(ch) => {
            if app.mode() == terminalvision::app::modes::Mode::CommandPalette {
                app.palette_push_char(ch);
            } else if app.mode() == terminalvision::app::modes::Mode::SmartJump {
                app.smart_jump_push_char(ch);
            } else {
                app.input_push_char(ch);
            }
        }
        InputEvent::ModalBackspace => {
            if app.mode() == terminalvision::app::modes::Mode::CommandPalette {
                app.palette_pop_char();
            } else if app.mode() == terminalvision::app::modes::Mode::SmartJump {
                app.smart_jump_pop_char();
            } else {
                app.input_pop_char();
            }
        }
        InputEvent::ModalConfirm => {
            if let Some(action) = app.confirm_modal() {
                app.handle_action(action);
            }
        }
        InputEvent::ModalMoveCursorLeft => {
            app.input_move_cursor_left();
        }
        InputEvent::ModalMoveCursorRight => {
            app.input_move_cursor_right();
        }
        InputEvent::ModalNavigateUp => {
            if app.mode() == terminalvision::app::modes::Mode::CommandPalette {
                app.palette_move_up();
            } else if app.mode() == terminalvision::app::modes::Mode::Bookmarks {
                app.bookmarks_mut().move_up();
            } else if app.mode() == terminalvision::app::modes::Mode::SmartJump {
                app.smart_jump_move_up();
            } else if app.mode() == terminalvision::app::modes::Mode::ProjectCockpit {
                app.project_cockpit_mut().move_up();
            } else if app.mode() == terminalvision::app::modes::Mode::GitStatusPanel {
                app.git_status_panel_mut().move_up();
            } else if app.mode() == terminalvision::app::modes::Mode::RevealContext {
                app.reveal_context_mut().move_up();
            }
        }
        InputEvent::ModalNavigateDown => {
            if app.mode() == terminalvision::app::modes::Mode::CommandPalette {
                app.palette_move_down();
            } else if app.mode() == terminalvision::app::modes::Mode::Bookmarks {
                app.bookmarks_mut().move_down();
            } else if app.mode() == terminalvision::app::modes::Mode::SmartJump {
                app.smart_jump_move_down();
            } else if app.mode() == terminalvision::app::modes::Mode::ProjectCockpit {
                app.project_cockpit_mut().move_down();
            } else if app.mode() == terminalvision::app::modes::Mode::GitStatusPanel {
                app.git_status_panel_mut().move_down();
            } else if app.mode() == terminalvision::app::modes::Mode::RevealContext {
                app.reveal_context_mut().move_down();
            }
        }
        InputEvent::ModalToggle => {
            if app.mode() == terminalvision::app::modes::Mode::Confirm {
                app.toggle_confirm_selection();
            }
        }
        InputEvent::ModalSetConfirm(val) => {
            if app.mode() == terminalvision::app::modes::Mode::Confirm {
                app.set_confirm_selection(val);
            }
        }
        InputEvent::Mouse(mouse) => {
            terminalvision::input::mouse::handle_mouse_event(
                mouse,
                app,
                size.area(),
                mouse_tracker,
            );
        }
        // Remembered for the rendering phase; the size is never assumed.
        InputEvent::Resize(reported) => *size = reported,
        InputEvent::Ignored => {}
    }
}

/// The terminal, while the application owns it.
///
/// This is the only place that changes terminal modes, so taking the terminal
/// and handing it back cannot drift apart.
struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    restored: bool,
}

impl TerminalSession {
    /// Takes over the terminal: raw mode first, then the alternate screen and mouse capture.
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;

        let mut stdout = io::stdout();
        if let Err(error) = execute!(
            stdout,
            EnterAlternateScreen,
            crossterm::event::EnableMouseCapture,
            crossterm::cursor::Hide
        ) {
            // Half of the takeover succeeded, so hand back what was taken
            // before reporting the failure.
            let _ = disable_raw_mode();
            return Err(error);
        }

        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(t) => t,
            Err(error) => {
                let _ = restore_terminal();
                return Err(error);
            }
        };

        Ok(Self {
            terminal,
            restored: false,
        })
    }

    /// Renders the prepared application state into the terminal.
    fn draw(&mut self, app: &App) -> io::Result<()> {
        self.terminal
            .draw(|frame| terminalvision::ui::render(frame, app))?;
        Ok(())
    }

    /// The size the terminal reports.
    fn size(&self) -> io::Result<TerminalSize> {
        crossterm::terminal::size().map(|(columns, rows)| TerminalSize::new(columns, rows))
    }

    /// Hands the terminal back.
    fn leave(mut self) -> io::Result<()> {
        self.restored = true;
        restore_terminal()
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        // Reached when an error unwinds out of the run, and by an early return
        // that never called `leave`. There is nobody left to report to here, so
        // the attempt is best effort.
        if !self.restored {
            let _ = restore_terminal();
        }
    }
}

/// Gives the terminal back: raw mode off, then the alternate screen left.
///
/// Best effort by design. The normal exit path reports its result; the panic
/// hook and [`Drop`] cannot, so they ignore it rather than losing the
/// restoration attempt. A panicking run reaches both of those, so the terminal
/// is restored twice: leaving an alternate screen that is already left is a
/// no-op, which makes the repeat harmless and keeps both paths independent.
fn restore_terminal() -> io::Result<()> {
    let raw_mode = disable_raw_mode();

    let mut stdout = io::stdout();
    let alternate_screen = execute!(
        stdout,
        crossterm::event::DisableMouseCapture,
        LeaveAlternateScreen,
        crossterm::cursor::Show
    )
    .and_then(|()| stdout.flush());

    raw_mode?;
    alternate_screen
}

/// Gives the terminal back before a panic is reported.
///
/// A hook runs before unwinding, so without this the report would be written to
/// the alternate screen and would vanish with it. The previous hook is called
/// afterwards, so the panic itself is still reported in full.
fn install_panic_hook() {
    let previous = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        // Nothing was taken over if the output is not a terminal.
        if io::stdout().is_terminal() {
            let _ = restore_terminal();
        }

        previous(panic_info);
    }));
}

#[cfg(test)]
mod tests {
    use super::{apply_input_event, start_application};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use terminalvision::app::actions::Action;
    use terminalvision::app::modes::Mode;
    use terminalvision::app::state::{ActivePane, App};
    use terminalvision::input::InputEvent;
    use terminalvision::input::mouse::MouseTracker;
    use terminalvision::layout::geometry::TerminalSize;

    const STARTING_SIZE: TerminalSize = TerminalSize::new(80, 24);

    #[test]
    fn an_action_event_reaches_the_application() {
        let mut app = App::default();
        let mut size = STARTING_SIZE;
        let mut tracker = MouseTracker::new();

        apply_input_event(
            &mut app,
            InputEvent::Action(Action::Quit),
            &mut size,
            &mut tracker,
        );

        assert!(app.should_quit());
    }

    #[test]
    fn the_loop_stops_once_the_application_asks_to_quit() {
        let mut app = App::default();
        let mut size = STARTING_SIZE;
        let mut tracker = MouseTracker::new();

        // A resize does not ask to stop, so the loop would carry on.
        apply_input_event(
            &mut app,
            InputEvent::Resize(TerminalSize::new(120, 30)),
            &mut size,
            &mut tracker,
        );
        assert!(!app.should_quit());

        // Only the quit request does.
        apply_input_event(
            &mut app,
            InputEvent::Action(Action::Quit),
            &mut size,
            &mut tracker,
        );
        assert!(app.should_quit());
    }

    #[test]
    fn a_resize_updates_the_size_and_leaves_the_application_alone() {
        let mut app = App::default();
        app.handle_action(Action::MoveDown);
        let expected = app.clone();
        let mut size = STARTING_SIZE;
        let mut tracker = MouseTracker::new();

        apply_input_event(
            &mut app,
            InputEvent::Resize(TerminalSize::new(132, 43)),
            &mut size,
            &mut tracker,
        );

        assert_eq!(
            size,
            TerminalSize::new(132, 43),
            "the reported size must be remembered exactly"
        );
        assert_eq!(
            app, expected,
            "terminal geometry must stay out of the application state"
        );
    }

    #[test]
    fn an_ignored_event_changes_nothing() {
        let mut app = App::default();
        app.handle_action(Action::MoveDown);
        let expected = app.clone();
        let mut size = STARTING_SIZE;
        let mut tracker = MouseTracker::new();

        apply_input_event(&mut app, InputEvent::Ignored, &mut size, &mut tracker);

        assert_eq!(app, expected);
        assert_eq!(size, STARTING_SIZE);
    }

    #[test]
    fn terminal_events_do_not_disturb_application_state() {
        // A whole session's worth of events, none of which is allowed to reach
        // into the application beyond the actions it was asked to apply.
        let mut app = App::default();
        let mut size = STARTING_SIZE;
        let mut tracker = MouseTracker::new();

        for event in [
            InputEvent::Resize(TerminalSize::new(40, 10)),
            InputEvent::Ignored,
            InputEvent::Resize(TerminalSize::new(200, 60)),
            InputEvent::Ignored,
        ] {
            apply_input_event(&mut app, event, &mut size, &mut tracker);
        }

        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.active_pane(), ActivePane::Left);
        assert_eq!(app.pane(ActivePane::Left).selected_index(), None);
        assert!(!app.should_quit());
        assert_eq!(size, TerminalSize::new(200, 60));
    }

    #[test]
    fn the_application_starts_in_the_working_directory() {
        let app = start_application().expect("the working directory should be readable");
        let working_directory =
            std::env::current_dir().expect("the test process has a working directory");

        assert_eq!(
            app.pane(ActivePane::Left).current_path(),
            &working_directory,
            "the runtime starts where the process was started"
        );
    }

    #[test]
    fn the_application_still_initialises_without_a_terminal() {
        let app = App::default();

        assert_eq!(app.mode(), Mode::Normal);
        assert!(!app.should_quit());
    }

    #[test]
    fn search_input_events_modify_search_query() {
        let mut app = App::default();
        let mut size = STARTING_SIZE;
        let mut tracker = MouseTracker::new();

        apply_input_event(
            &mut app,
            InputEvent::Action(Action::StartSearch),
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.mode(), Mode::Search);

        apply_input_event(
            &mut app,
            InputEvent::SearchChar('f'),
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::SearchChar('o'),
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::SearchChar('o'),
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.search().query(), "foo");

        apply_input_event(
            &mut app,
            InputEvent::SearchBackspace,
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.search().query(), "fo");

        apply_input_event(&mut app, InputEvent::SearchConfirm, &mut size, &mut tracker);
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.search().query(), "fo");
    }

    #[test]
    fn mouse_events_are_processed_via_input_event() {
        let mut app = App::default();
        let mut size = STARTING_SIZE;
        let mut tracker = MouseTracker::new();

        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 10,
            row: 5,
            modifiers: KeyModifiers::NONE,
        };

        apply_input_event(
            &mut app,
            InputEvent::Mouse(mouse_event),
            &mut size,
            &mut tracker,
        );
        // Does not panic and preserves mode
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn command_palette_modal_input_events() {
        let mut app = App::default();
        let mut size = STARTING_SIZE;
        let mut tracker = MouseTracker::new();

        apply_input_event(
            &mut app,
            InputEvent::Action(Action::CommandPalette),
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.mode(), Mode::CommandPalette);

        apply_input_event(
            &mut app,
            InputEvent::ModalChar('q'),
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::ModalChar('u'),
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::ModalChar('i'),
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::ModalChar('t'),
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.command_palette().query(), "quit");

        apply_input_event(
            &mut app,
            InputEvent::ModalBackspace,
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.command_palette().query(), "qui");

        // Navigate down/up
        apply_input_event(
            &mut app,
            InputEvent::ModalNavigateDown,
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::ModalNavigateUp,
            &mut size,
            &mut tracker,
        );

        // Confirm executing "Quit"
        apply_input_event(
            &mut app,
            InputEvent::ModalChar('t'),
            &mut size,
            &mut tracker,
        );
        apply_input_event(&mut app, InputEvent::ModalConfirm, &mut size, &mut tracker);
        assert_eq!(app.mode(), Mode::Normal);
        assert!(app.should_quit());
    }

    #[test]
    fn input_dialog_modal_events() {
        let mut app = App::default();
        let mut size = STARTING_SIZE;
        let mut tracker = MouseTracker::new();

        apply_input_event(
            &mut app,
            InputEvent::Action(Action::NewFile),
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.mode(), Mode::Create);

        apply_input_event(
            &mut app,
            InputEvent::ModalChar('t'),
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::ModalChar('e'),
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::ModalChar('s'),
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::ModalChar('t'),
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.input_buffer(), "test");
        assert_eq!(app.cursor_position(), 4);

        apply_input_event(
            &mut app,
            InputEvent::ModalMoveCursorLeft,
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.cursor_position(), 3);

        apply_input_event(
            &mut app,
            InputEvent::ModalMoveCursorRight,
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.cursor_position(), 4);

        apply_input_event(
            &mut app,
            InputEvent::ModalBackspace,
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.input_buffer(), "tes");
        assert_eq!(app.cursor_position(), 3);

        // Cancel modal
        apply_input_event(
            &mut app,
            InputEvent::Action(Action::Cancel),
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn confirm_dialog_modal_events() {
        let mut app = App::default();
        let mut size = STARTING_SIZE;
        let mut tracker = MouseTracker::new();

        // Simulate confirm mode
        app.handle_action(Action::NewFile); // enter temp mode
        apply_input_event(
            &mut app,
            InputEvent::Action(Action::Cancel),
            &mut size,
            &mut tracker,
        );

        // Test modal toggle and set confirm
        assert!(!app.confirm_selection());
        apply_input_event(
            &mut app,
            InputEvent::ModalSetConfirm(true),
            &mut size,
            &mut tracker,
        );
        // In Normal mode, modal events shouldn't affect confirm_selection
        assert!(!app.confirm_selection());
    }

    #[test]
    fn bookmark_modal_input_events() {
        let mut app = App::default();
        let mut size = STARTING_SIZE;
        let mut tracker = MouseTracker::new();

        app.bookmarks_mut().add(std::path::PathBuf::from("/test/a"));
        app.bookmarks_mut().add(std::path::PathBuf::from("/test/b"));

        apply_input_event(
            &mut app,
            InputEvent::Action(Action::OpenBookmarks),
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.mode(), Mode::Bookmarks);
        assert_eq!(app.bookmarks().selected_index(), 0);

        apply_input_event(
            &mut app,
            InputEvent::ModalNavigateDown,
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.bookmarks().selected_index(), 1);

        apply_input_event(
            &mut app,
            InputEvent::ModalNavigateUp,
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.bookmarks().selected_index(), 0);

        // Cancel
        apply_input_event(
            &mut app,
            InputEvent::Action(Action::Cancel),
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn smart_jump_modal_input_events() {
        let mut app = App::default();
        let mut size = STARTING_SIZE;
        let mut tracker = MouseTracker::new();

        apply_input_event(
            &mut app,
            InputEvent::Action(Action::SmartJump),
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.mode(), Mode::SmartJump);

        apply_input_event(
            &mut app,
            InputEvent::ModalChar('h'),
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::ModalChar('o'),
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::ModalChar('m'),
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::ModalChar('e'),
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.smart_jump().query(), "home");

        apply_input_event(
            &mut app,
            InputEvent::ModalBackspace,
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.smart_jump().query(), "hom");

        apply_input_event(
            &mut app,
            InputEvent::ModalNavigateDown,
            &mut size,
            &mut tracker,
        );
        apply_input_event(
            &mut app,
            InputEvent::ModalNavigateUp,
            &mut size,
            &mut tracker,
        );

        // Cancel
        apply_input_event(
            &mut app,
            InputEvent::Action(Action::Cancel),
            &mut size,
            &mut tracker,
        );
        assert_eq!(app.mode(), Mode::Normal);
    }
}
