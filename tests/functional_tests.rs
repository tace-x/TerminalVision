//! End-to-end functional and interaction tests for TerminalVision.
//!
//! Validates:
//! 1. Startup path resolution and dual-pane initialization.
//! 2. Enter key navigation on directories, safe preview on files, symlink handling.
//! 3. Text input system (Search, Rename, Create File/Dir, Path Jump, Command Palette).
//! 4. Full modal lifecycle (Home, End, Delete, Backspace, Esc, Enter).
//! 5. File operations on real filesystem (Create, Rename, Copy, Cut, Paste, Delete).
//! 6. Live preview updates on selection and pane switching.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use terminalvision::app::actions::Action;
use terminalvision::app::modes::Mode;
use terminalvision::app::state::{ActivePane, App, CreateKind};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_dual_pane_startup_initialization() {
    let temp = TempDir::new("tv-dual-pane-startup");
    let file1 = temp.path().join("alpha.txt");
    let file2 = temp.path().join("beta.rs");
    let dir1 = temp.path().join("gamma_dir");
    fs::write(&file1, b"alpha content").unwrap();
    fs::write(&file2, b"fn main() {}").unwrap();
    fs::create_dir(&dir1).unwrap();

    let app =
        App::at(temp.path().to_path_buf()).expect("Application should initialize at directory");

    // Both panes must be initialized and point to the valid directory
    assert_eq!(app.active_pane(), ActivePane::Left);
    assert_eq!(app.pane(ActivePane::Left).current_path(), temp.path());
    assert_eq!(app.pane(ActivePane::Right).current_path(), temp.path());

    // Both panes must have loaded real entries
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 3);
    assert_eq!(app.pane(ActivePane::Right).entries().len(), 3);

    // Left pane has initial selection
    assert_eq!(app.pane(ActivePane::Left).selected_index(), Some(0));
    assert_eq!(app.pane(ActivePane::Right).selected_index(), Some(0));

    // Right pane preview is populated with live preview of the first item
    assert!(app.preview().content().is_some());
}

#[test]
fn test_enter_key_directory_navigation_and_file_safety() {
    let temp = TempDir::new("tv-enter-nav");
    let sub = temp.path().join("sub_folder");
    let file = temp.path().join("code.rs");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("nested.txt"), b"nested content").unwrap();
    fs::write(&file, b"pub fn hello() {}").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    // Select sub_folder (which sorts after code.rs or before depending on sort mode)
    let which = app.active_pane();
    app.pane_mut(which).select_name(OsStr::new("sub_folder"));

    // Press Enter (Action::Open) on directory -> navigates into directory
    app.handle_action(Action::Open);
    assert_eq!(app.pane(ActivePane::Left).current_path(), &sub);
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);
    assert_eq!(app.pane(ActivePane::Left).entries()[0].name(), "nested.txt");

    // Backspace (Action::GoParent) -> returns to parent directory
    app.handle_action(Action::GoParent);
    assert_eq!(app.pane(ActivePane::Left).current_path(), temp.path());

    // Select regular file (code.rs) and press Enter -> does NOT execute, updates preview
    app.pane_mut(which).select_name(OsStr::new("code.rs"));
    app.handle_action(Action::Open);
    assert_eq!(app.pane(ActivePane::Left).current_path(), temp.path()); // path unchanged
    assert_eq!(app.preview().path(), Some(file.as_path()));
    let text = app.preview().content().unwrap().as_text().unwrap();
    assert_eq!(text.lines(), &["pub fn hello() {}"]);
}

#[test]
fn test_text_input_system_search_live_filter() {
    let temp = TempDir::new("tv-search-test");
    fs::write(temp.path().join("alpha_test.txt"), b"1").unwrap();
    fs::write(temp.path().join("beta_test.rs"), b"2").unwrap();
    fs::write(temp.path().join("gamma_data.json"), b"3").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    // Press '/' -> StartSearch
    app.handle_action(Action::StartSearch);
    assert_eq!(app.mode(), Mode::Search);

    // Type "beta"
    app.set_search_query("beta");
    assert_eq!(app.search().query(), "beta");
    assert_eq!(app.pane(ActivePane::Left).visible_count(), 1);
    let visible: Vec<_> = app.pane(ActivePane::Left).visible_entries().collect();
    assert_eq!(visible[0].name(), "beta_test.rs");

    // Backspace to "bet"
    app.set_search_query("bet");
    assert_eq!(app.pane(ActivePane::Left).visible_count(), 1);

    // Confirm search with Enter -> returns to Normal mode with filter active
    app.confirm_search();
    assert_eq!(app.mode(), Mode::Normal);
    assert_eq!(app.pane(ActivePane::Left).visible_count(), 1);

    // Clear search with Action::ClearSearch or Esc in Search mode -> resets list
    app.handle_action(Action::ClearSearch);
    assert_eq!(app.mode(), Mode::Normal);
    assert_eq!(app.pane(ActivePane::Left).visible_count(), 3);
}

#[test]
fn test_text_input_system_rename_file() {
    let temp = TempDir::new("tv-rename-test");
    let old_file = temp.path().join("old_name.txt");
    fs::write(&old_file, b"content to rename").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.pane_mut(ActivePane::Left)
        .select_name(OsStr::new("old_name.txt"));

    // Press 'r' -> Rename dialog
    app.handle_action(Action::Rename);
    assert_eq!(app.mode(), Mode::Rename);
    assert_eq!(app.input_buffer(), "old_name.txt");

    // Clear input buffer and type "new_name.txt"
    app.input_move_cursor_home();
    // Simulate typing new name
    app.set_input_buffer("new_name.txt");

    // Press Enter to confirm modal
    let action = app.confirm_modal();
    assert!(action.is_none());
    assert_eq!(app.mode(), Mode::Normal);

    // Real filesystem verification
    assert!(!old_file.exists());
    assert!(temp.path().join("new_name.txt").exists());

    // Active pane reloaded and selected the renamed file
    let selected = app.pane(ActivePane::Left).selected_entry().unwrap();
    assert_eq!(selected.name(), "new_name.txt");
}

#[test]
fn test_text_input_system_create_file_and_directory() {
    let temp = TempDir::new("tv-create-test");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    // 1. Create File ('n')
    app.handle_action(Action::NewFile);
    assert_eq!(app.mode(), Mode::Create);
    assert_eq!(app.create_kind(), Some(CreateKind::File));

    app.set_input_buffer("created_file.rs");
    app.confirm_modal();
    assert_eq!(app.mode(), Mode::Normal);
    assert!(temp.path().join("created_file.rs").exists());

    // 2. Create Directory ('N')
    app.handle_action(Action::NewDirectory);
    assert_eq!(app.mode(), Mode::Create);
    assert_eq!(app.create_kind(), Some(CreateKind::Directory));

    app.set_input_buffer("created_folder");
    app.confirm_modal();
    assert_eq!(app.mode(), Mode::Normal);
    assert!(temp.path().join("created_folder").is_dir());
}

#[test]
fn test_text_input_system_path_jump() {
    let temp = TempDir::new("tv-jump-test");
    let target_dir = temp.path().join("deep").join("nested").join("target");
    fs::create_dir_all(&target_dir).unwrap();
    fs::write(target_dir.join("inner.txt"), b"hello").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    // Press 'g' -> Jump dialog
    app.handle_action(Action::JumpToPath);
    assert_eq!(app.mode(), Mode::Jump);

    let jump_path = target_dir.to_string_lossy().to_string();
    app.set_input_buffer(&jump_path);
    app.confirm_modal();

    assert_eq!(app.mode(), Mode::Normal);
    assert_eq!(app.pane(ActivePane::Left).current_path(), &target_dir);
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);
}

#[test]
fn test_text_input_system_command_palette() {
    let temp = TempDir::new("tv-palette-test");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    // Press Ctrl+P -> Command Palette
    app.handle_action(Action::CommandPalette);
    assert_eq!(app.mode(), Mode::CommandPalette);

    // Type "sort" into palette search query
    app.palette_push_char('s');
    app.palette_push_char('o');
    app.palette_push_char('r');
    app.palette_push_char('t');

    assert_eq!(app.command_palette().query(), "sort");
    let filtered = app.command_palette().filtered_commands();
    assert!(!filtered.is_empty());
    assert!(filtered.iter().any(|c| c.action() == Action::ChangeSort));

    // Confirm execution of selected command
    let action = app.confirm_modal();
    assert!(action.is_some());
    assert_eq!(app.mode(), Mode::Normal);
}

#[test]
fn test_cursor_editing_home_end_delete() {
    let temp = TempDir::new("tv-cursor-test");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    app.handle_action(Action::NewFile);
    app.input_push_char('a');
    app.input_push_char('b');
    app.input_push_char('c');
    assert_eq!(app.input_buffer(), "abc");
    assert_eq!(app.cursor_position(), 3);

    app.input_move_cursor_home();
    assert_eq!(app.cursor_position(), 0);

    app.input_push_char('z');
    assert_eq!(app.input_buffer(), "zabc");
    assert_eq!(app.cursor_position(), 1);

    app.input_move_cursor_end();
    assert_eq!(app.cursor_position(), 4);

    app.input_push_char('!');
    assert_eq!(app.input_buffer(), "zabc!");

    app.input_move_cursor_home();
    app.input_delete_char(); // deletes 'z'
    assert_eq!(app.input_buffer(), "abc!");
}

#[test]
fn test_file_operations_copy_cut_paste_delete() {
    let temp = TempDir::new("tv-file-ops-test");
    let source_dir = temp.path().join("source");
    let dest_dir = temp.path().join("dest");
    fs::create_dir(&source_dir).unwrap();
    fs::create_dir(&dest_dir).unwrap();

    let sample_file = source_dir.join("sample.txt");
    fs::write(&sample_file, b"sample content").unwrap();

    let mut app = App::at(source_dir.clone()).unwrap();
    app.open_in(ActivePane::Right, dest_dir.clone()).unwrap();

    // 1. Copy sample.txt
    app.pane_mut(ActivePane::Left)
        .select_name(OsStr::new("sample.txt"));
    app.handle_action(Action::Copy);
    assert!(!app.clipboard().is_empty());

    // Switch to right pane and Paste
    app.handle_action(Action::SwitchPane);
    assert_eq!(app.active_pane(), ActivePane::Right);
    app.handle_action(Action::Paste);

    // Real filesystem verification
    assert!(source_dir.join("sample.txt").exists());
    assert!(dest_dir.join("sample.txt").exists());

    // 2. Delete pasted file in right pane
    app.pane_mut(ActivePane::Right)
        .select_name(OsStr::new("sample.txt"));
    app.handle_action(Action::Delete);
    assert!(!dest_dir.join("sample.txt").exists());
}

#[test]
fn test_sorting_and_hidden_files_toggles() {
    let temp = TempDir::new("tv-sort-hidden-test");
    fs::write(temp.path().join(".hidden_file"), b"hidden").unwrap();
    fs::write(temp.path().join("a_file.txt"), b"1").unwrap();
    fs::write(temp.path().join("z_file.txt"), b"22222").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    // By default, hidden files are not shown (2 visible)
    assert_eq!(app.pane(ActivePane::Left).visible_count(), 2);

    // Toggle hidden files on (3 visible)
    app.handle_action(Action::ToggleHidden);
    assert_eq!(app.pane(ActivePane::Left).visible_count(), 3);

    // Toggle hidden files back off (2 visible)
    app.handle_action(Action::ToggleHidden);
    assert_eq!(app.pane(ActivePane::Left).visible_count(), 2);

    // Cycle sort mode
    let initial_sort = app.pane(ActivePane::Left).sort_mode();
    app.handle_action(Action::ChangeSort);
    let new_sort = app.pane(ActivePane::Left).sort_mode();
    assert_ne!(initial_sort, new_sort);
}

#[test]
fn test_integrated_terminal_lifecycle_and_focus_switching() {
    let temp = TempDir::new("tv-term-lifecycle");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    // Initialize terminal with dimensions 80x20
    app.init_terminal(80, 20);

    // Initial state: Mode::Normal
    assert_eq!(app.mode(), Mode::Normal);

    // Toggle focus to Terminal
    app.handle_action(Action::ToggleTerminalFocus);
    assert_eq!(app.mode(), Mode::Terminal);

    // Send key to terminal (e.g. echo hello)
    app.terminal_send_bytes(b"echo hello\n");

    // Scroll terminal scrollback
    app.handle_action(Action::ScrollTerminalUp);
    app.handle_action(Action::ScrollTerminalDown);
    app.terminal_reset_scroll();

    // Resize terminal
    app.terminal_resize(120, 30);
    assert!(!app.terminal_shell_name().is_empty());

    // Switch focus back to File Manager
    app.handle_action(Action::FocusFileManager);
    assert_eq!(app.mode(), Mode::Normal);

    // Explicitly focus terminal
    app.handle_action(Action::FocusTerminal);
    assert_eq!(app.mode(), Mode::Terminal);

    // Toggle back to Normal
    app.handle_action(Action::ToggleTerminalFocus);
    assert_eq!(app.mode(), Mode::Normal);
}

#[test]
fn test_terminal_file_manager_directory_synchronization() {
    let temp = TempDir::new("tv-term-sync");
    let sub = temp.path().join("sub_folder");
    fs::create_dir(&sub).unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.init_terminal(80, 20);

    // 1. Sync Terminal to Active Pane Directory
    app.handle_action(Action::SyncTerminalToDirectory);

    // 2. Simulate new file created in terminal
    let new_file = temp.path().join("from_terminal.txt");
    fs::write(&new_file, b"created via terminal command").unwrap();

    // Refresh directory
    app.handle_action(Action::RefreshDirectory);
    assert!(
        app.pane(ActivePane::Left)
            .entries()
            .iter()
            .any(|e| e.name() == "from_terminal.txt")
    );

    // 3. Sync Directory to Terminal
    app.handle_action(Action::SyncDirectoryToTerminal);
    assert!(app.pane(ActivePane::Left).current_path().exists());
}

#[test]
fn test_image_preview_decoding_and_metadata() {
    use terminalvision::preview::image::ImageFormat;
    use terminalvision::preview::{PreviewContent, load_preview};

    let temp = TempDir::new("tv-image-preview");

    // 1. PNG Image with valid header & IHDR chunk (32x16, 8-bit RGBA)
    let png_path = temp.path().join("test.png");
    let mut png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG Signature
    // IHDR chunk: Length (13), "IHDR", Width (32), Height (16), Depth (8), ColorType (6 = RGBA), Compression (0), Filter (0), Interlace (0), CRC
    png_data.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R', 0x00, 0x00, 0x00, 0x20, // 32
        0x00, 0x00, 0x00, 0x10, // 16
        0x08, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]);
    fs::write(&png_path, &png_data).unwrap();

    let preview = load_preview(&png_path);
    if let PreviewContent::Image(img) = preview {
        assert_eq!(img.format, ImageFormat::Png);
        assert_eq!(img.width, 32);
        assert_eq!(img.height, 16);
        assert_eq!(img.aspect_ratio_str(), "2.00:1 (32×16)");
    } else {
        panic!("expected PreviewContent::Image for PNG, got {preview:?}");
    }

    // 2. GIF Image (800x600, GIF89a)
    let gif_path = temp.path().join("anim.gif");
    let mut gif_data = b"GIF89a".to_vec();
    gif_data.extend_from_slice(&[0x20, 0x03, 0x58, 0x02, 0x80, 0x00, 0x00]); // width: 800, height: 600
    fs::write(&gif_path, &gif_data).unwrap();

    let preview = load_preview(&gif_path);
    if let PreviewContent::Image(img) = preview {
        assert_eq!(img.format, ImageFormat::Gif);
        assert_eq!(img.width, 800);
        assert_eq!(img.height, 600);
        assert_eq!(img.aspect_ratio_str(), "4:3");
    } else {
        panic!("expected PreviewContent::Image for GIF, got {preview:?}");
    }

    // 3. BMP Image (1920x1080, BM header)
    let bmp_path = temp.path().join("photo.bmp");
    let mut bmp_data = vec![b'B', b'M'];
    bmp_data.resize(18, 0); // pad to DIB header width
    bmp_data.extend_from_slice(&[0x80, 0x07, 0x00, 0x00]); // width = 1920
    bmp_data.extend_from_slice(&[0x38, 0x04, 0x00, 0x00]); // height = 1080
    bmp_data.extend_from_slice(&[0x01, 0x00]); // planes = 1
    bmp_data.extend_from_slice(&[0x18, 0x00]); // bpp = 24
    fs::write(&bmp_path, &bmp_data).unwrap();

    let preview = load_preview(&bmp_path);
    if let PreviewContent::Image(img) = preview {
        assert_eq!(img.format, ImageFormat::Bmp);
        assert_eq!(img.width, 1920);
        assert_eq!(img.height, 1080);
        assert_eq!(img.aspect_ratio_str(), "16:9");
    } else {
        panic!("expected PreviewContent::Image for BMP, got {preview:?}");
    }

    // 4. Corrupted / truncated image file -> gracefully decodes fallback by extension without crash
    let corrupt_png = temp.path().join("corrupt.png");
    fs::write(&corrupt_png, b"corrupted bytes").unwrap();
    let preview = load_preview(&corrupt_png);
    assert!(matches!(
        preview,
        PreviewContent::Image(_) | PreviewContent::Binary | PreviewContent::Error(_)
    ));
}

#[test]
fn test_unicode_internationalization_support() {
    let temp = TempDir::new("tv-unicode-support");
    let filenames = [
        "മലയാളം_രേഖ.txt",                    // Malayalam
        "日本語のファイル.rs",              // Japanese
        "中文文档_项目.md",                 // Chinese
        "ملف_عربي_بيانات.json",             // Arabic
        "Café_and_Résumé_2026.pdf",         // Accented Latin
        "⚡_TerminalVision_🚀_🦀.rs",       // Emojis
        "spaced filename with symbols.txt", // Spaces
    ];

    for name in filenames {
        fs::write(temp.path().join(name), format!("Content of {name}")).unwrap();
    }

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    assert_eq!(app.pane(ActivePane::Left).entries().len(), filenames.len());

    // Navigation and selection through all Unicode entries
    for _ in 0..filenames.len() {
        app.handle_action(Action::MoveDown);
        let sel = app.pane(ActivePane::Left).selected_entry();
        assert!(sel.is_some());
    }

    // Test Search on Malayalam filename
    app.handle_action(Action::StartSearch);
    app.set_search_query("മലയാളം");
    assert_eq!(app.pane(ActivePane::Left).visible_count(), 1);
    let matched = app.pane(ActivePane::Left).visible_entries().next().unwrap();
    assert_eq!(matched.name(), "മലയാളം_രേഖ.txt");
    app.handle_action(Action::ClearSearch);

    // Test Search on Emoji
    app.handle_action(Action::StartSearch);
    app.set_search_query("🚀");
    assert_eq!(app.pane(ActivePane::Left).visible_count(), 1);
    let matched = app.pane(ActivePane::Left).visible_entries().next().unwrap();
    assert_eq!(matched.name(), "⚡_TerminalVision_🚀_🦀.rs");
    app.handle_action(Action::ClearSearch);
}

#[test]
fn test_extreme_terminal_dimensions_no_panic() {
    use ratatui::layout::Rect;
    use terminalvision::layout::geometry::ScreenLayout;

    let test_sizes = [
        (240, 80),
        (200, 60),
        (160, 40),
        (120, 30),
        (100, 30),
        (80, 24),
        (60, 15),
        (40, 10),
        (20, 5),
        (1, 1),
    ];

    for (cols, rows) in test_sizes {
        let area = Rect::new(0, 0, cols, rows);
        let layout = ScreenLayout::calculate(area);

        assert!(layout.header().height <= area.height);
        assert!(layout.terminal().height <= area.height);
        assert!(layout.footer().height <= area.height);
        assert!(layout.terminal().y >= layout.header().y);
        assert!(layout.footer().y >= layout.terminal().y);
    }
}
