//! Exhaustive test suite for Unified Terminal <-> File Manager State (Phase 1.2).
//!
//! Covers all 28 required test scenarios:
//! 1. startup cwd
//! 2. startup with directory argument
//! 3. startup with file argument
//! 4. terminal pwd matches File Manager
//! 5. File Manager navigation -> terminal cwd
//! 6. terminal cd -> File Manager
//! 7. cd ..
//! 8. cd ~
//! 9. cd /
//! 10. absolute paths
//! 11. relative paths
//! 12. paths with spaces
//! 13. Unicode paths
//! 14. invalid cd
//! 15. mkdir from terminal
//! 16. touch from terminal
//! 17. rm from terminal
//! 18. mv from terminal
//! 19. cp from terminal
//! 20. File Manager create
//! 21. File Manager rename
//! 22. File Manager delete
//! 23. active pane synchronization
//! 24. shell restart
//! 25. deleted current directory
//! 26. preview invalidation
//! 27. Git state refresh
//! 28. synchronization loop prevention

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use terminalvision::app::actions::Action;
use terminalvision::app::state::{ActivePane, App, SyncOrigin};
use terminalvision::utils::path::{
    expand_home, filesystem_root, resolve_target_path, safe_fallback_directory,
};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
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

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_01_startup_cwd() {
    let app = App::at_working_directory().expect("App must start in current working directory");
    let cwd = terminalvision::filesystem::FilesystemService::new()
        .current_directory()
        .unwrap();
    assert_eq!(app.pane(ActivePane::Left).current_path(), &cwd);
    assert_eq!(app.active_location().path, cwd);
    assert!(app.active_location().synchronized);
}

#[test]
fn test_02_startup_with_directory_argument() {
    let temp = TestDir::new("tv-test-02-dir-arg");
    let sub = temp.path().join("sub_project");
    fs::create_dir(&sub).unwrap();

    let resolved = resolve_target_path(&sub.to_string_lossy(), temp.path());
    let app = App::at(resolved.clone()).expect("App must start at target directory");

    assert_eq!(app.pane(ActivePane::Left).current_path(), &sub);
    assert_eq!(app.active_location().path, sub);
    assert!(app.active_location().synchronized);
}

#[test]
fn test_03_startup_with_file_argument() {
    let temp = TestDir::new("tv-test-03-file-arg");
    let file = temp.path().join("test_file.rs");
    fs::write(&file, b"fn main() {}").unwrap();

    let resolved = resolve_target_path(&file.to_string_lossy(), temp.path());
    let parent = resolved.parent().unwrap().to_path_buf();

    let mut app = App::at(parent.clone()).expect("App must start at parent directory");
    app.active_pane_mut()
        .select_name(OsStr::new("test_file.rs"));
    app.refresh_preview();

    assert_eq!(app.pane(ActivePane::Left).current_path(), &parent);
    assert_eq!(
        app.pane(ActivePane::Left)
            .selected_entry()
            .map(|e| e.name()),
        Some(OsStr::new("test_file.rs"))
    );
    assert_eq!(app.active_location().path, parent);
}

#[test]
fn test_04_terminal_pwd_matches_file_manager() {
    let temp = TestDir::new("tv-test-04-pwd-match");
    let app = App::at(temp.path().to_path_buf()).unwrap();
    app.init_terminal(80, 24);

    std::thread::sleep(std::time::Duration::from_millis(50));

    assert_eq!(app.pane(app.active_pane()).current_path(), temp.path());
    assert_eq!(
        app.terminal_cwd().canonicalize().unwrap(),
        temp.path().canonicalize().unwrap()
    );
    assert!(app.active_location().synchronized);
}

#[test]
fn test_05_file_manager_navigation_to_terminal_cwd() {
    let temp = TestDir::new("tv-test-05-fm-to-term");
    let sub = temp.path().join("child_dir");
    fs::create_dir(&sub).unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.init_terminal(80, 24);

    // Navigate to sub directory in File Manager
    app.open_in(ActivePane::Left, sub.clone()).unwrap();

    assert_eq!(app.pane(ActivePane::Left).current_path(), &sub);
    assert_eq!(app.last_sync_origin(), SyncOrigin::FileManagerNavigation);
}

#[test]
fn test_06_terminal_cd_to_file_manager() {
    let temp = TestDir::new("tv-test-06-term-to-fm");
    let sub = temp.path().join("documents");
    fs::create_dir(&sub).unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.init_terminal(80, 24);

    // Simulate shell process CWD change (e.g. user ran cd documents in terminal)
    let changed = app.handle_shell_cwd_change(sub.clone());
    assert!(changed);
    assert_eq!(app.pane(ActivePane::Left).current_path(), &sub);
    assert_eq!(app.last_sync_origin(), SyncOrigin::ShellCwdChange);
}

#[test]
fn test_07_cd_dot_dot() {
    let temp = TestDir::new("tv-test-07-cd-dotdot");
    let sub = temp.path().join("level1").join("level2");
    fs::create_dir_all(&sub).unwrap();

    let mut app = App::at(sub.clone()).unwrap();
    app.init_terminal(80, 24);

    // cd .. in terminal
    let parent = sub.parent().unwrap().to_path_buf();
    let changed = app.handle_shell_cwd_change(parent.clone());
    assert!(changed);
    assert_eq!(app.pane(ActivePane::Left).current_path(), &parent);
}

#[test]
fn test_08_cd_tilde() {
    let home = expand_home("~");
    if home.exists() && home.is_dir() {
        let temp = TestDir::new("tv-test-08-cd-tilde");
        let mut app = App::at(temp.path().to_path_buf()).unwrap();
        app.init_terminal(80, 24);

        let changed = app.handle_shell_cwd_change(home.clone());
        assert!(changed);
        assert_eq!(app.pane(ActivePane::Left).current_path(), &home);
    }
}

#[test]
fn test_09_cd_root() {
    let root = filesystem_root(Path::new("/"));
    if root.exists() && root.is_dir() {
        let temp = TestDir::new("tv-test-09-cd-root");
        let mut app = App::at(temp.path().to_path_buf()).unwrap();
        app.init_terminal(80, 24);

        let changed = app.handle_shell_cwd_change(root.clone());
        assert!(changed);
        assert_eq!(app.pane(ActivePane::Left).current_path(), &root);
    }
}

#[test]
fn test_10_absolute_paths() {
    let temp = TestDir::new("tv-test-10-abs-paths");
    let dir_a = temp.path().join("dir_a");
    let dir_b = temp.path().join("dir_b");
    fs::create_dir(&dir_a).unwrap();
    fs::create_dir(&dir_b).unwrap();

    let mut app = App::at(dir_a.clone()).unwrap();
    let resolved = resolve_target_path(&dir_b.to_string_lossy(), &dir_a);
    assert_eq!(resolved, dir_b);

    app.open_in(ActivePane::Left, resolved).unwrap();
    assert_eq!(app.pane(ActivePane::Left).current_path(), &dir_b);
}

#[test]
fn test_11_relative_paths() {
    let temp = TestDir::new("tv-test-11-rel-paths");
    let child = temp.path().join("child");
    fs::create_dir(&child).unwrap();

    let resolved = resolve_target_path("child", temp.path());
    assert_eq!(resolved, child);

    let resolved_parent = resolve_target_path("../", &child);
    assert_eq!(resolved_parent, temp.path());
}

#[test]
fn test_12_paths_with_spaces() {
    let temp = TestDir::new("tv-test-12-spaces");
    let space_dir = temp.path().join("My Projects and Documents");
    fs::create_dir(&space_dir).unwrap();
    fs::write(space_dir.join("hello world.txt"), b"data").unwrap();

    let app = App::at(space_dir.clone()).unwrap();
    app.init_terminal(80, 24);

    assert_eq!(app.pane(ActivePane::Left).current_path(), &space_dir);
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);
    assert_eq!(
        app.pane(ActivePane::Left).entries()[0].name(),
        "hello world.txt"
    );
}

#[test]
fn test_13_unicode_paths() {
    let temp = TestDir::new("tv-test-13-unicode");
    let unicode_dir = temp.path().join("🦀_Rust_日本語_folder");
    fs::create_dir(&unicode_dir).unwrap();
    fs::write(unicode_dir.join("ファイル.txt"), b"unicode content").unwrap();

    let app = App::at(unicode_dir.clone()).unwrap();
    app.init_terminal(80, 24);

    assert_eq!(app.pane(ActivePane::Left).current_path(), &unicode_dir);
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);
}

#[test]
fn test_14_invalid_cd() {
    let temp = TestDir::new("tv-test-14-invalid-cd");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.init_terminal(80, 24);

    let invalid_path = temp.path().join("non_existent_folder_xyz");
    let changed = app.handle_shell_cwd_change(invalid_path);

    // FM must NOT follow invalid path; state must remain at valid temp directory
    assert!(!changed);
    assert_eq!(app.pane(ActivePane::Left).current_path(), temp.path());
}

#[test]
fn test_15_mkdir_from_terminal() {
    let temp = TestDir::new("tv-test-15-mkdir");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 0);

    // Simulate terminal running mkdir NewFolder
    let new_folder = temp.path().join("NewFolder");
    fs::create_dir(&new_folder).unwrap();

    // Trigger watcher check
    let changes = app
        .filesystem_watcher_mut()
        .check_changes_now(&[temp.path().to_path_buf()], None);
    assert!(!changes.is_empty());

    app.refresh_directory();
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);
    assert_eq!(app.pane(ActivePane::Left).entries()[0].name(), "NewFolder");
}

#[test]
fn test_16_touch_from_terminal() {
    let temp = TestDir::new("tv-test-16-touch");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    // Simulate terminal running touch file.txt
    let file = temp.path().join("file.txt");
    fs::write(&file, b"").unwrap();

    app.refresh_directory();
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);
    assert_eq!(app.pane(ActivePane::Left).entries()[0].name(), "file.txt");
}

#[test]
fn test_17_rm_from_terminal() {
    let temp = TestDir::new("tv-test-17-rm");
    let file = temp.path().join("doomed.txt");
    fs::write(&file, b"content").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);

    // Simulate terminal running rm doomed.txt
    fs::remove_file(&file).unwrap();

    app.refresh_directory();
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 0);
}

#[test]
fn test_18_mv_from_terminal() {
    let temp = TestDir::new("tv-test-18-mv");
    let file1 = temp.path().join("old.txt");
    let file2 = temp.path().join("new.txt");
    fs::write(&file1, b"content").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    assert_eq!(app.pane(ActivePane::Left).entries()[0].name(), "old.txt");

    // Simulate terminal running mv old.txt new.txt
    fs::rename(&file1, &file2).unwrap();

    app.refresh_directory();
    assert_eq!(app.pane(ActivePane::Left).entries()[0].name(), "new.txt");
}

#[test]
fn test_19_cp_from_terminal() {
    let temp = TestDir::new("tv-test-19-cp");
    let src = temp.path().join("source.txt");
    let dst = temp.path().join("copied.txt");
    fs::write(&src, b"source content").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);

    // Simulate terminal running cp source.txt copied.txt
    fs::copy(&src, &dst).unwrap();

    app.refresh_directory();
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 2);
}

#[test]
fn test_20_file_manager_create() {
    let temp = TestDir::new("tv-test-20-fm-create");
    let mut app = App::at(temp.path().to_path_buf()).unwrap();

    app.create_file(OsStr::new("created_by_fm.txt")).unwrap();
    assert!(temp.path().join("created_by_fm.txt").exists());
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 1);

    app.create_directory(OsStr::new("fm_folder")).unwrap();
    assert!(temp.path().join("fm_folder").is_dir());
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 2);
}

#[test]
fn test_21_file_manager_rename() {
    let temp = TestDir::new("tv-test-21-fm-rename");
    let file = temp.path().join("initial.txt");
    fs::write(&file, b"content").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.active_pane_mut().select_name(OsStr::new("initial.txt"));

    app.rename_selected(OsStr::new("renamed.txt")).unwrap();
    assert!(!temp.path().join("initial.txt").exists());
    assert!(temp.path().join("renamed.txt").exists());
}

#[test]
fn test_22_file_manager_delete() {
    let temp = TestDir::new("tv-test-22-fm-delete");
    let file = temp.path().join("to_delete.txt");
    fs::write(&file, b"data").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.active_pane_mut()
        .select_name(OsStr::new("to_delete.txt"));

    app.delete_selected().unwrap();
    assert!(!temp.path().join("to_delete.txt").exists());
    assert_eq!(app.pane(ActivePane::Left).entries().len(), 0);
}

#[test]
fn test_23_active_pane_synchronization() {
    let temp = TestDir::new("tv-test-23-pane-sync");
    let left_dir = temp.path().join("left_folder");
    let right_dir = temp.path().join("right_folder");
    fs::create_dir(&left_dir).unwrap();
    fs::create_dir(&right_dir).unwrap();

    let mut app = App::at(left_dir.clone()).unwrap();
    app.open_in(ActivePane::Right, right_dir.clone()).unwrap();
    app.init_terminal(80, 24);

    assert_eq!(app.active_pane(), ActivePane::Left);
    assert_eq!(app.pane(ActivePane::Left).current_path(), &left_dir);
    assert_eq!(app.active_location().path, left_dir);

    // Switch to Right Pane
    app.handle_action(Action::SwitchPane);
    assert_eq!(app.active_pane(), ActivePane::Right);
    assert_eq!(app.pane(ActivePane::Right).current_path(), &right_dir);
    assert_eq!(app.active_location().path, right_dir);
    assert_eq!(app.last_sync_origin(), SyncOrigin::ActivePaneSwitch);
}

#[test]
fn test_24_shell_restart() {
    let temp = TestDir::new("tv-test-24-shell-restart");
    let sub = temp.path().join("active_work");
    fs::create_dir(&sub).unwrap();

    let app = App::at(sub.clone()).unwrap();
    app.init_terminal(80, 24);
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Reinitializing terminal must preserve the active directory
    app.init_terminal(80, 24);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(
        app.terminal_cwd().canonicalize().unwrap(),
        sub.canonicalize().unwrap()
    );
}

#[test]
fn test_25_deleted_current_directory() {
    let temp = TestDir::new("tv-test-25-deleted-dir");
    let doomed_dir = temp.path().join("will_be_deleted");
    fs::create_dir(&doomed_dir).unwrap();

    let fallback = safe_fallback_directory(&doomed_dir);
    assert_eq!(fallback, temp.path());

    // Delete the directory
    fs::remove_dir(&doomed_dir).unwrap();
    let fallback_after_deletion = safe_fallback_directory(&doomed_dir);
    assert!(fallback_after_deletion.exists());
}

#[test]
fn test_26_preview_invalidation() {
    let temp = TestDir::new("tv-test-26-preview-inv");
    let file = temp.path().join("preview_target.txt");
    fs::write(&file, b"initial preview content").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.active_pane_mut()
        .select_name(OsStr::new("preview_target.txt"));
    app.refresh_preview();

    assert!(app.preview().content().is_some());

    // Modify file on disk with different length and content
    fs::write(&file, b"updated preview content with new longer text!").unwrap();

    // Check watcher detects preview file modification
    let changes = app
        .filesystem_watcher_mut()
        .check_changes_now(&[temp.path().to_path_buf()], Some(&file));
    assert!(changes.iter().any(|c| matches!(
        c,
        terminalvision::filesystem::FilesystemChange::PreviewFileModified(_)
    )));

    app.refresh_preview();
    assert!(app.preview().content().is_some());
}

#[test]
fn test_27_git_state_refresh() {
    let temp = TestDir::new("tv-test-27-git-refresh");
    let git_dir = temp.path().join(".git");
    fs::create_dir(&git_dir).unwrap();
    fs::write(git_dir.join("HEAD"), b"ref: refs/heads/main\n").unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.filesystem_watcher_mut()
        .watch(&[temp.path().to_path_buf()], None);

    // Simulate git checkout or branch update
    fs::write(git_dir.join("HEAD"), b"ref: refs/heads/feature\n").unwrap();

    let changes = app
        .filesystem_watcher_mut()
        .check_changes_now(&[temp.path().to_path_buf()], None);
    assert!(changes.iter().any(|c| matches!(
        c,
        terminalvision::filesystem::FilesystemChange::GitStateChanged(_)
    )));
}

#[test]
fn test_28_synchronization_loop_prevention() {
    let temp = TestDir::new("tv-test-28-loop-prevention");
    let sub1 = temp.path().join("sub1");
    let sub2 = temp.path().join("sub2");
    fs::create_dir(&sub1).unwrap();
    fs::create_dir(&sub2).unwrap();

    let mut app = App::at(temp.path().to_path_buf()).unwrap();
    app.init_terminal(80, 24);

    // 1. FM navigates -> sets origin to FileManagerNavigation
    app.open_in(ActivePane::Left, sub1.clone()).unwrap();
    assert_eq!(app.last_sync_origin(), SyncOrigin::FileManagerNavigation);

    // 2. Shell detects matching directory -> does not trigger redundant navigation
    let changed_same = app.handle_shell_cwd_change(sub1.clone());
    assert!(!changed_same);

    // 3. Shell changes to sub2 -> sets origin to ShellCwdChange without looping
    let changed_diff = app.handle_shell_cwd_change(sub2.clone());
    assert!(changed_diff);
    assert_eq!(app.last_sync_origin(), SyncOrigin::ShellCwdChange);
    assert_eq!(app.pane(ActivePane::Left).current_path(), &sub2);
}

#[test]
fn test_manual_test_scenario_full_flow() {
    let temp = TestDir::new("tv-manual-test-flow");
    let downloads = temp.path().join("Downloads");
    let projects = downloads.join("Projects");
    fs::create_dir_all(&projects).unwrap();

    // 1. Launch terminalvision ~/Downloads
    let mut app = App::at(downloads.clone()).unwrap();
    app.filesystem_watcher_mut()
        .set_min_poll_interval(std::time::Duration::from_millis(0));
    app.init_terminal(80, 24);
    std::thread::sleep(std::time::Duration::from_millis(50));

    assert_eq!(app.pane(ActivePane::Left).current_path(), &downloads);
    assert_eq!(
        app.terminal_cwd().canonicalize().unwrap(),
        downloads.canonicalize().unwrap()
    );

    // 2. Navigate File Manager into ~/Downloads/Projects
    app.open_in(ActivePane::Left, projects.clone()).unwrap();
    assert_eq!(app.pane(ActivePane::Left).current_path(), &projects);

    // 3. Inside terminal: cd ..
    let changed = app.handle_shell_cwd_change(downloads.clone());
    assert!(changed);
    assert_eq!(app.pane(ActivePane::Left).current_path(), &downloads);

    // 4. mkdir TV_SYNC_TEST && touch TV_SYNC_TEST/hello.txt
    let sync_test_dir = downloads.join("TV_SYNC_TEST");
    fs::create_dir(&sync_test_dir).unwrap();
    fs::write(sync_test_dir.join("hello.txt"), b"hello").unwrap();

    let updated = app.poll_sync_and_filesystem();
    assert!(updated);
    assert!(
        app.pane(ActivePane::Left)
            .entries()
            .iter()
            .any(|e| e.name() == "TV_SYNC_TEST")
    );

    // 5. rm TV_SYNC_TEST/hello.txt && rmdir TV_SYNC_TEST
    fs::remove_file(sync_test_dir.join("hello.txt")).unwrap();
    fs::remove_dir(&sync_test_dir).unwrap();

    let updated2 = app.poll_sync_and_filesystem();
    assert!(updated2);
    assert!(
        !app.pane(ActivePane::Left)
            .entries()
            .iter()
            .any(|e| e.name() == "TV_SYNC_TEST")
    );

    // 6. Test cd /tmp, cd ~, cd .., cd "folder with spaces"
    let tmp = PathBuf::from("/tmp");
    if tmp.exists() && tmp.is_dir() {
        let changed = app.handle_shell_cwd_change(tmp.clone());
        assert!(changed);
        assert_eq!(
            app.pane(ActivePane::Left)
                .current_path()
                .canonicalize()
                .unwrap(),
            tmp.canonicalize().unwrap()
        );
    }

    let home = expand_home("~");
    if home.exists() && home.is_dir() {
        let changed = app.handle_shell_cwd_change(home.clone());
        assert!(changed);
        assert_eq!(app.pane(ActivePane::Left).current_path(), &home);
    }

    let space_dir = temp.path().join("folder with spaces");
    fs::create_dir(&space_dir).unwrap();
    let changed = app.handle_shell_cwd_change(space_dir.clone());
    assert!(changed);
    assert_eq!(app.pane(ActivePane::Left).current_path(), &space_dir);
}
