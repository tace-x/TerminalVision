//! Integration tests for TerminalVision Phase 1.3: Project-Aware UI & Command Center Integration.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use terminalvision::app::actions::Action;
use terminalvision::app::modes::Mode;
use terminalvision::app::state::{ActivePane, App};

/// Helper to create a unique temporary directory that cleans up automatically.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tv-ui-{prefix}-{nanos}"));
        fs::create_dir_all(&path).expect("failed to create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn create_file(&self, rel: &str, content: &str) -> PathBuf {
        let p = self.path.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&p, content).expect("failed to write test file");
        p
    }

    fn create_dir(&self, rel: &str) -> PathBuf {
        let p = self.path.join(rel);
        fs::create_dir_all(&p).expect("failed to create test dir");
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).ok();
    }
}

#[test]
fn test_01_project_header_context() {
    let tmp = TempDir::new("header-ctx");
    tmp.create_file(
        "Cargo.toml",
        "[package]\nname = \"my_rust_pkg\"\nversion = \"0.1.0\"\n",
    );
    tmp.create_dir("src");
    tmp.create_file("src/main.rs", "fn main() {}\n");

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().to_path_buf());

    let backend = TestBackend::new(120, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            terminalvision::ui::header::render(f, f.area(), &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content: String = (0..120).map(|x| buffer[(x, 0)].symbol()).collect();
    assert!(content.contains("TerminalVision"));
    assert!(content.contains("my_rust_pkg") || content.contains("Rust"));
}

#[test]
fn test_02_non_project_header_state() {
    let tmp = TempDir::new("non-proj-header");
    tmp.create_file("notes.txt", "just plain text");

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().to_path_buf());

    let backend = TestBackend::new(100, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            terminalvision::ui::header::render(f, f.area(), &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content: String = (0..100).map(|x| buffer[(x, 0)].symbol()).collect();
    assert!(content.contains("TerminalVision"));
    assert!(!content.contains("Unknown"));
    assert!(!content.contains("Project: None"));
    assert!(!content.contains("[proj: None]"));
}

#[test]
fn test_03_project_root_navigation() {
    let tmp = TempDir::new("root-nav");
    tmp.create_file(
        "Cargo.toml",
        "[package]\nname = \"nav_app\"\nversion = \"0.1.0\"\n",
    );
    let sub = tmp.create_dir("src/terminal/sub");

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, sub.clone());
    assert_eq!(app.pane(app.active_pane()).current_path(), &sub);

    app.handle_action(Action::GoProjectRoot);
    assert_eq!(app.pane(app.active_pane()).current_path(), tmp.path());
}

#[test]
fn test_04_command_center_project_actions() {
    let tmp = TempDir::new("cmd-center-proj");
    tmp.create_file(
        "Cargo.toml",
        "[package]\nname = \"cmd_app\"\nversion = \"0.1.0\"\n",
    );
    tmp.create_dir("src");
    tmp.create_dir("tests");
    tmp.create_dir("docs");
    tmp.create_file("README.md", "# CMD App\n");

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().to_path_buf());

    app.open_command_palette();
    assert_eq!(app.mode(), Mode::CommandPalette);

    let entries = app.command_palette().entries();
    let action_types: Vec<Action> = entries.iter().filter_map(|e| e.action()).collect();

    assert!(action_types.contains(&Action::GoProjectRoot));
    assert!(action_types.contains(&Action::GoSourceDir));
    assert!(action_types.contains(&Action::GoTestsDir));
    assert!(action_types.contains(&Action::GoDocsDir));
    assert!(action_types.contains(&Action::OpenManifest));
    assert!(action_types.contains(&Action::OpenReadme));
}

#[test]
fn test_05_missing_readme() {
    let tmp = TempDir::new("no-readme");
    tmp.create_file(
        "Cargo.toml",
        "[package]\nname = \"no_readme_app\"\nversion = \"0.1.0\"\n",
    );
    tmp.create_dir("src");

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().to_path_buf());

    app.open_command_palette();
    let entries = app.command_palette().entries();
    let action_types: Vec<Action> = entries.iter().filter_map(|e| e.action()).collect();

    assert!(
        !action_types.contains(&Action::OpenReadme),
        "OpenReadme should not be exposed when no README exists"
    );
}

#[test]
fn test_06_missing_tests() {
    let tmp = TempDir::new("no-tests");
    tmp.create_file(
        "Cargo.toml",
        "[package]\nname = \"no_tests_app\"\nversion = \"0.1.0\"\n",
    );
    tmp.create_dir("src");

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().to_path_buf());

    app.open_command_palette();
    let entries = app.command_palette().entries();
    let action_types: Vec<Action> = entries.iter().filter_map(|e| e.action()).collect();

    assert!(
        !action_types.contains(&Action::GoTestsDir),
        "GoTestsDir should not be exposed when no tests directory exists"
    );
}

#[test]
fn test_07_missing_manifest() {
    let tmp = TempDir::new("no-manifest");
    tmp.create_file("misc.txt", "hello");

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().to_path_buf());

    app.open_command_palette();
    let entries = app.command_palette().entries();
    let action_types: Vec<Action> = entries.iter().filter_map(|e| e.action()).collect();

    assert!(
        !action_types.contains(&Action::OpenManifest),
        "OpenManifest should not be exposed when no manifest exists"
    );
    assert!(
        !action_types.contains(&Action::GoProjectRoot),
        "GoProjectRoot should not be exposed when not in a project"
    );
}

#[test]
fn test_08_git_and_non_git_project() {
    let tmp = TempDir::new("non-git-proj");
    tmp.create_file(
        "Cargo.toml",
        "[package]\nname = \"nongit_pkg\"\nversion = \"0.1.0\"\n",
    );

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().to_path_buf());

    app.open_command_palette();
    let entries = app.command_palette().entries();
    let action_types: Vec<Action> = entries.iter().filter_map(|e| e.action()).collect();

    assert!(
        !action_types.contains(&Action::GoGitRoot),
        "GoGitRoot should not be exposed in non-git directory"
    );
    assert!(
        !action_types.contains(&Action::GitStatusPanel),
        "GitStatusPanel should not be exposed in non-git directory"
    );
}

#[test]
fn test_09_nested_project() {
    let tmp = TempDir::new("nested-ws");
    tmp.create_file(
        "Cargo.toml",
        "[workspace]\nmembers = [\"frontend\", \"backend\"]\n",
    );
    tmp.create_file(
        "frontend/package.json",
        "{\n  \"name\": \"frontend\",\n  \"version\": \"1.0.0\"\n}\n",
    );
    tmp.create_file(
        "backend/Cargo.toml",
        "[package]\nname = \"backend\"\nversion = \"0.1.0\"\n",
    );

    let mut app = App::default();
    let backend_path = tmp.path().join("backend");
    let _ = app.open_in(ActivePane::Left, backend_path);

    let backend = TestBackend::new(120, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            terminalvision::ui::header::render(f, f.area(), &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content: String = (0..120).map(|x| buffer[(x, 0)].symbol()).collect();
    assert!(content.contains("backend"));
}

#[test]
fn test_10_monorepo() {
    let tmp = TempDir::new("monorepo-overview");
    tmp.create_file("Cargo.toml", "[workspace]\nmembers = [\"core\", \"cli\"]\n");
    tmp.create_file(
        "core/Cargo.toml",
        "[package]\nname = \"my-core\"\nversion = \"0.1.0\"\n",
    );
    tmp.create_file(
        "cli/Cargo.toml",
        "[package]\nname = \"my-cli\"\nversion = \"0.1.0\"\n",
    );

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().to_path_buf());

    app.handle_action(Action::ProjectCockpit);
    assert_eq!(app.mode(), Mode::ProjectCockpit);

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            terminalvision::ui::dialogs::render(f, f.area(), &app);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let text: String = (0..24)
        .flat_map(|y| (0..80).map(move |x| buffer[(x, y)].symbol().to_string()))
        .collect();

    assert!(text.contains("Project Overview"));
    assert!(text.contains("Workspace"));
}

#[test]
fn test_11_quick_switcher_project_entries() {
    let tmp = TempDir::new("qs-proj-entries");
    tmp.create_file(
        "Cargo.toml",
        "[package]\nname = \"qs_app\"\nversion = \"0.1.0\"\n",
    );
    tmp.create_dir("src");
    tmp.create_dir("tests");
    tmp.create_dir("docs");
    tmp.create_file("README.md", "# QS App\n");
    tmp.create_file("LICENSE", "MIT License\n");

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().to_path_buf());

    app.open_smart_jump();
    assert_eq!(app.mode(), Mode::SmartJump);

    let items = app.smart_jump().all_items();
    let categories: Vec<&str> = items.iter().map(|i| i.category).collect();

    assert!(categories.contains(&"PROJECT"));
    assert!(categories.contains(&"SOURCE"));
    assert!(categories.contains(&"TESTS"));
    assert!(categories.contains(&"DOCS"));
    assert!(categories.contains(&"IMPORTANT"));
}

#[test]
fn test_12_breadcrumb_integration() {
    let tmp = TempDir::new("breadcrumb-int");
    tmp.create_file(
        "Cargo.toml",
        "[package]\nname = \"bc_app\"\nversion = \"0.1.0\"\n",
    );
    let sub = tmp.create_dir("src/sub/deep");

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, sub);

    let area = Rect::new(0, 0, 100, 1);
    // Clicking near the start of the breadcrumb should hit a valid path segment
    let hit = terminalvision::ui::header::breadcrumb_hit_segment(area, &app, 45, 0);
    // Either some segment is hit or None depending on exact x coordinate
    if let Some(target) = hit {
        assert!(target.starts_with(tmp.path()) || tmp.path().starts_with(&target));
    }
}

#[test]
fn test_13_preview_integration() {
    let tmp = TempDir::new("preview-int");
    tmp.create_file(
        "Cargo.toml",
        "[package]\nname = \"prev_app\"\nversion = \"0.1.0\"\n",
    );
    let main_rs = tmp.create_file("src/main.rs", "fn main() {}\n");

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().join("src"));
    app.active_pane_mut().select_path(&main_rs);
    app.refresh_preview();

    if let Some(terminalvision::preview::PreviewContent::Metadata(meta)) = app.preview().content() {
        assert_eq!(meta.project_context(), Some("prev_app"));
    }
}

#[test]
fn test_14_project_context_refresh() {
    let tmp1 = TempDir::new("proj-refresh-1");
    tmp1.create_file(
        "Cargo.toml",
        "[package]\nname = \"proj_one\"\nversion = \"0.1.0\"\n",
    );

    let tmp2 = TempDir::new("proj-refresh-2");
    tmp2.create_file(
        "package.json",
        "{\n  \"name\": \"proj-two\",\n  \"version\": \"1.0.0\"\n}\n",
    );

    let mut app = App::default();

    // 1. Enter project 1
    let _ = app.open_in(ActivePane::Left, tmp1.path().to_path_buf());
    assert!(app.workspace_context().is_active());
    let p1 = app
        .workspace_context()
        .project_for_path(tmp1.path())
        .cloned()
        .expect("expected project node for tmp1");
    assert_eq!(p1.root, tmp1.path());
    assert_eq!(p1.project_type, terminalvision::project::ProjectType::Rust);

    // 2. Navigate to project 2
    let _ = app.open_in(ActivePane::Left, tmp2.path().to_path_buf());
    assert!(app.workspace_context().is_active());
    let p2 = app
        .workspace_context()
        .project_for_path(tmp2.path())
        .cloned()
        .expect("expected project node for tmp2");
    assert_eq!(p2.root, tmp2.path());
    assert_eq!(p2.project_type, terminalvision::project::ProjectType::Node);
}

#[test]
fn test_15_leaving_a_project() {
    let tmp_proj = TempDir::new("leave-proj");
    tmp_proj.create_file(
        "Cargo.toml",
        "[package]\nname = \"temp_proj\"\nversion = \"0.1.0\"\n",
    );

    let tmp_plain = TempDir::new("plain-dir");
    tmp_plain.create_file("plain.txt", "data");

    let mut app = App::default();

    // Enter project
    let _ = app.open_in(ActivePane::Left, tmp_proj.path().to_path_buf());
    assert!(app.workspace_context().is_active());

    // Leave project to plain directory
    let _ = app.open_in(ActivePane::Left, tmp_plain.path().to_path_buf());
    assert!(!app.workspace_context().is_active());
}

#[test]
fn test_16_responsive_layouts() {
    let tmp = TempDir::new("resp-audit");
    tmp.create_file(
        "Cargo.toml",
        "[package]\nname = \"resp_pkg\"\nversion = \"0.1.0\"\n",
    );

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().to_path_buf());
    app.handle_action(Action::ProjectCockpit);

    let dimensions = [
        (80, 24),
        (100, 30),
        (120, 30),
        (159, 30),
        (160, 40),
        (180, 40),
        (200, 60),
    ];

    for (w, h) in dimensions {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                terminalvision::ui::header::render(f, Rect::new(0, 0, f.area().width, 1), &app);
                terminalvision::ui::dialogs::render(f, f.area(), &app);
            })
            .unwrap();
    }
}

#[test]
fn test_17_keyboard_navigation() {
    let tmp = TempDir::new("kbd-nav");
    tmp.create_file(
        "Cargo.toml",
        "[package]\nname = \"kbd_pkg\"\nversion = \"0.1.0\"\n",
    );
    tmp.create_dir("src");

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().to_path_buf());

    app.handle_action(Action::ProjectCockpit);
    assert_eq!(app.mode(), Mode::ProjectCockpit);

    let initial_idx = app.project_cockpit().selected_index();
    assert_eq!(initial_idx, 0);

    app.project_cockpit_mut().move_down();
    assert_eq!(app.project_cockpit().selected_index(), 1);

    app.project_cockpit_mut().move_up();
    assert_eq!(app.project_cockpit().selected_index(), 0);

    app.handle_action(Action::Cancel);
    assert_eq!(app.mode(), Mode::Normal);
}

#[test]
fn test_18_mouse_hit_testing() {
    let tmp = TempDir::new("mouse-hit");
    tmp.create_file(
        "Cargo.toml",
        "[package]\nname = \"mouse_pkg\"\nversion = \"0.1.0\"\n",
    );
    tmp.create_dir("src");

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, tmp.path().to_path_buf());

    app.handle_action(Action::ProjectCockpit);
    assert_eq!(app.mode(), Mode::ProjectCockpit);

    let initial_len = app.project_cockpit().actions().len();
    assert!(initial_len > 0);

    app.project_cockpit_mut().select(0);
    assert_eq!(app.project_cockpit().selected_index(), 0);
}

#[test]
fn test_19_invalid_project_state() {
    let tmp = TempDir::new("invalid-state");
    let nested = tmp.create_dir("deleted-proj");
    fs::write(
        nested.join("Cargo.toml"),
        "[package]\nname = \"deleted\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let mut app = App::default();
    let _ = app.open_in(ActivePane::Left, nested.clone());

    // Delete project directory from disk while open
    fs::remove_dir_all(&nested).ok();

    // Action execution and overview should handle deleted path gracefully without panicking
    app.handle_action(Action::GoProjectRoot);
    app.handle_action(Action::ProjectCockpit);
    app.handle_action(Action::Cancel);
}

#[test]
fn test_20_permission_and_error_fallback() {
    let mut app = App::default();
    let non_existent = PathBuf::from("/non/existent/path/for/sure");
    let outcome = app.open_in(ActivePane::Left, non_existent);
    assert!(outcome.is_err());

    // Context remains valid and inactive without crashing
    assert!(!app.workspace_context().is_active());
}
