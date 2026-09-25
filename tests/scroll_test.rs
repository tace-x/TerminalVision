use terminalvision::app::actions::Action;
use terminalvision::app::state::{ActivePane, App};

#[test]
fn test_scroll_crash() {
    let mut app = App::at(std::env::current_dir().unwrap()).unwrap();
    app.set_visible_rows(ActivePane::Left, 10);
    app.set_visible_rows(ActivePane::Right, 10);

    // PageDown should cause the crash if visible_rows is used.
    for _ in 0..100 {
        app.handle_action(Action::PageDown);
    }
    for _ in 0..100 {
        app.handle_action(Action::PageUp);
    }
}
