use std::fs;
use tempfile::TempDir;
use terminalvision::app::actions::Action;
use terminalvision::app::state::{ActivePane, App};

#[test]
fn test_scroll_fuzz() {
    let temp = TempDir::new().unwrap();
    for i in 0..100 {
        fs::write(temp.path().join(format!("file_{}.txt", i)), b"").unwrap();
    }
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    // Fuzz
    for r in 0..100 {
        app.set_visible_rows(ActivePane::Left, r % 15);
        app.handle_action(Action::MoveDown);
        app.handle_action(Action::PageDown);
    }
}
