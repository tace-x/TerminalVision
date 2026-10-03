//! Comprehensive integration tests for Phase 2.3:
//! Smart Breadcrumb + Favorites / Quick Access + Storage Vision & Heatmaps.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use tempfile::tempdir;

use terminalvision::app::actions::Action;
use terminalvision::app::modes::Mode;
use terminalvision::app::state::{ActivePane, App};
use terminalvision::filesystem::classification::FileCategory;
use terminalvision::navigation::{FavoriteItem, FavoriteList, SmartBreadcrumb};
use terminalvision::storage::{
    StorageScanner, StorageVisionState, format_storage_size, render_storage_bar,
};

#[test]
fn test_01_breadcrumb_root_home_and_nested_segments() {
    // 1. Root path
    let root = Path::new("/");
    let root_bc = SmartBreadcrumb::from_path(root);
    assert!(!root_bc.segments.is_empty());
    assert_eq!(root_bc.segments[0].name, "/");
    assert!(root_bc.segments[0].is_current);

    // 2. Custom nested path
    let nested = Path::new("/var/log/nginx/access.log");
    let nested_bc = SmartBreadcrumb::from_path(nested);
    assert!(nested_bc.segments.len() >= 4);
    assert_eq!(nested_bc.segments.last().unwrap().name, "access.log");
    assert!(nested_bc.segments.last().unwrap().is_current);

    // 3. Spaced vs Compact width formatting
    let wide_str = nested_bc.format_for_width(200);
    assert!(wide_str.contains(" / "));

    let narrow_str = nested_bc.format_for_width(20);
    assert!(!narrow_str.is_empty());
    assert!(narrow_str.len() <= 25);
}

#[test]
fn test_02_breadcrumb_header_hit_segment_navigation() {
    let tmp = tempdir().unwrap();
    let sub1 = tmp.path().join("projects");
    let sub2 = sub1.join("terminalvision");
    let sub3 = sub2.join("src");
    fs::create_dir_all(&sub3).unwrap();

    let mut app = App::at(sub3.clone()).unwrap();
    let header_area = Rect::new(0, 0, 120, 1);

    // Hit-testing segment
    let hit = terminalvision::ui::header::breadcrumb_hit_segment(header_area, &app, 18, 0);
    if let Some(target) = hit {
        assert!(target.exists());
    }

    // Direct segment click navigation in app
    let outcome = app.open_in(ActivePane::Left, sub1.clone());
    assert!(outcome.is_ok());
    assert_eq!(app.pane(ActivePane::Left).current_path(), &sub1);
}

#[test]
fn test_03_favorites_crud_and_reordering() {
    let mut favorites = FavoriteList::new();
    let p1 = PathBuf::from("/tmp/projects");
    let p2 = PathBuf::from("/tmp/downloads");
    let p3 = PathBuf::from("/tmp/documents");

    // Add
    assert!(favorites.add_with_name("Projects".to_string(), p1.clone()));
    assert!(favorites.add_with_name("Downloads".to_string(), p2.clone()));
    assert!(favorites.add_with_name("Documents".to_string(), p3.clone()));
    assert_eq!(favorites.len(), 3);

    // Duplicate prevention
    assert!(!favorites.add_with_name("Duplicate Projects".to_string(), p1.clone()));
    assert_eq!(favorites.len(), 3);

    // Reorder: Move index 1 down
    assert!(favorites.move_down(1));
    assert_eq!(favorites.items()[2].path, p2);

    // Move index 2 up
    assert!(favorites.move_up(2));
    assert_eq!(favorites.items()[1].path, p2);

    // Remove
    let removed = favorites.remove_at(1);
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().path, p2);
    assert_eq!(favorites.len(), 2);
}

#[test]
fn test_04_favorites_disk_validation_and_broken_paths() {
    let tmp = tempdir().unwrap();
    let valid_dir = tmp.path().join("valid_folder");
    fs::create_dir_all(&valid_dir).unwrap();
    let missing_dir = tmp.path().join("non_existent_folder");

    let item_valid = FavoriteItem::new(1, "Valid".to_string(), valid_dir);
    let item_missing = FavoriteItem::new(2, "Broken".to_string(), missing_dir);

    assert!(item_valid.is_valid());
    assert!(!item_missing.is_valid());

    assert_eq!(item_valid.display_label(), "★ Valid");
    assert_eq!(item_missing.display_label(), "⚠ Broken (missing)");
}

#[test]
fn test_05_favorites_persistence_roundtrip() {
    let tmp = tempdir().unwrap();
    let mut app = App::at(tmp.path().to_path_buf()).unwrap();

    let fav_path = tmp.path().join("saved_favorite");
    fs::create_dir_all(&fav_path).unwrap();

    app.bookmarks_mut().add(fav_path.clone());
    assert!(app.bookmarks().contains_path(&fav_path));

    // Export settings
    let settings = app.to_settings();
    assert!(settings.bookmarks.iter().any(|b| b.path == fav_path));

    // Reapply to a new App instance
    let mut fresh_app = App::at(tmp.path().to_path_buf()).unwrap();
    fresh_app.apply_persistent_settings(&settings);
    assert!(fresh_app.bookmarks().contains_path(&fav_path));
}

#[test]
fn test_06_storage_units_formatting() {
    assert_eq!(format_storage_size(0), "0 B");
    assert_eq!(format_storage_size(999), "999 B");
    assert_eq!(format_storage_size(1_000), "1.0 KB");
    assert_eq!(format_storage_size(15_400_000), "15.4 MB");
    assert_eq!(format_storage_size(42_800_000_000), "42.8 GB");
    assert_eq!(format_storage_size(3_200_000_000_000), "3.2 TB");
}

#[test]
fn test_07_storage_heatmap_bar_rendering() {
    let bar_full = render_storage_bar(100.0, 10);
    assert_eq!(bar_full, "██████████");

    let bar_half = render_storage_bar(50.0, 10);
    assert_eq!(bar_half, "█████░░░░░");

    let bar_empty = render_storage_bar(0.0, 10);
    assert_eq!(bar_empty, "░░░░░░░░░░");
}

#[test]
fn test_08_storage_scanner_directory_aggregation_and_classification() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Create subfolder with source code
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let mut f1 = File::create(src_dir.join("main.rs")).unwrap();
    f1.write_all(&vec![b'a'; 10_000]).unwrap(); // 10 KB

    // Create subfolder with archive
    let archive_dir = root.join("archives");
    fs::create_dir_all(&archive_dir).unwrap();
    let mut f2 = File::create(archive_dir.join("data.zip")).unwrap();
    f2.write_all(&vec![b'b'; 25_000]).unwrap(); // 25 KB

    let cancel = Arc::new(AtomicBool::new(false));
    let items = Arc::new(AtomicUsize::new(0));
    let bytes = Arc::new(AtomicU64::new(0));

    let scanner = StorageScanner::new(cancel, items, bytes);
    let analysis = scanner.scan_directory(root);

    assert_eq!(analysis.total_files, 2);
    assert_eq!(analysis.total_dirs, 2);
    assert!(analysis.total_bytes >= 35_000);
    assert!(analysis.is_complete);

    // Verify categories
    let has_source = analysis
        .category_breakdown
        .iter()
        .any(|(cat, _, _)| *cat == FileCategory::SourceCode);
    let has_archive = analysis
        .category_breakdown
        .iter()
        .any(|(cat, _, _)| *cat == FileCategory::Archive);
    assert!(has_source);
    assert!(has_archive);

    // Verify direct children
    assert_eq!(analysis.direct_children.len(), 2);
    let total_pct: f32 = analysis.direct_children.iter().map(|c| c.percentage).sum();
    assert!((total_pct - 100.0).abs() < 1.0);
}

#[test]
fn test_09_storage_scanner_cancellation() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    for i in 0..50 {
        let dir = root.join(format!("dir_{i}"));
        fs::create_dir_all(&dir).unwrap();
        let mut f = File::create(dir.join("file.bin")).unwrap();
        f.write_all(&vec![b'x'; 5_000]).unwrap();
    }

    let cancel = Arc::new(AtomicBool::new(true)); // Pre-cancelled
    let items = Arc::new(AtomicUsize::new(0));
    let bytes = Arc::new(AtomicU64::new(0));

    let scanner = StorageScanner::new(cancel, items, bytes);
    let analysis = scanner.scan_directory(root);

    assert!(!analysis.is_complete);
}

#[test]
fn test_10_storage_scanner_symlink_safety() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    let sub = root.join("sub");
    fs::create_dir_all(&sub).unwrap();

    let mut f = File::create(sub.join("test.txt")).unwrap();
    f.write_all(b"hello").unwrap();

    #[cfg(unix)]
    {
        // Create cyclic symlink loop: sub/loop -> root
        let loop_link = sub.join("loop");
        let _ = std::os::unix::fs::symlink(root, loop_link);
    }

    let cancel = Arc::new(AtomicBool::new(false));
    let items = Arc::new(AtomicUsize::new(0));
    let bytes = Arc::new(AtomicU64::new(0));

    let scanner = StorageScanner::new(cancel, items, bytes);
    let analysis = scanner.scan_directory(root);

    assert!(analysis.is_complete);
    assert!(analysis.total_files >= 1);
}

#[test]
fn test_11_storage_vision_drill_down_and_history_navigation() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let child_dir = root.join("child_a");
    fs::create_dir_all(&child_dir).unwrap();
    let mut f = File::create(child_dir.join("data.txt")).unwrap();
    f.write_all(b"content").unwrap();

    let mut state = StorageVisionState::new();
    state.start_scan(root.clone());

    // Wait briefly for local thread scan
    let start = Instant::now();
    while state.is_scanning && start.elapsed() < Duration::from_millis(500) {
        state.poll_updates();
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(state.analysis.is_some());
    assert_eq!(state.current_scope, root);

    // Drill down
    let drilled = state.drill_down();
    assert!(drilled);
    assert_eq!(state.history_stack.len(), 1);
    assert_eq!(state.history_stack[0], root);

    // Go back
    let backed = state.go_back();
    assert!(backed);
    assert_eq!(state.history_stack.len(), 0);
    assert_eq!(state.current_scope, root);
}

#[test]
fn test_12_responsive_dialog_rendering_across_all_resolutions() {
    let tmp = tempdir().unwrap();
    let mut app = App::at(tmp.path().to_path_buf()).unwrap();
    app.handle_action(Action::StorageVision);
    assert_eq!(app.mode(), Mode::StorageVision);

    let resolutions = [
        (80, 24),
        (100, 30),
        (120, 30),
        (159, 30),
        (160, 40),
        (180, 40),
        (200, 60),
    ];

    for (w, h) in resolutions {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                terminalvision::ui::render(f, &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width == w && buffer.area.height == h);
    }
}
