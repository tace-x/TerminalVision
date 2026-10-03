//! Filesystem watcher and change detection layer.
//!
//! Tracks directory timestamps, entry counts, and preview file status
//! to detect filesystem mutations (e.g. from terminal commands or external tools)
//! without heavy background threads or resource-intensive polling.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Represents a detected change on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesystemChange {
    /// Directory entries modified, added, or removed.
    DirectoryModified(PathBuf),
    /// Directory was deleted or moved.
    DirectoryDeleted(PathBuf),
    /// The previewed file was modified.
    PreviewFileModified(PathBuf),
    /// The previewed file was deleted.
    PreviewFileDeleted(PathBuf),
    /// Git repository state changed.
    GitStateChanged(PathBuf),
}

/// Directory fingerprint snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DirectorySnapshot {
    mtime: Option<SystemTime>,
    entry_count: usize,
    exists: bool,
}

/// File fingerprint snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSnapshot {
    mtime: Option<SystemTime>,
    len: u64,
    exists: bool,
}

/// Watches targeted directories and files for changes.
#[derive(Debug, Clone)]
pub struct FilesystemWatcher {
    directory_snapshots: HashMap<PathBuf, DirectorySnapshot>,
    file_snapshots: HashMap<PathBuf, FileSnapshot>,
    git_snapshots: HashMap<PathBuf, Option<SystemTime>>,
    last_poll: Option<Instant>,
    min_poll_interval: Duration,
}

impl PartialEq for FilesystemWatcher {
    fn eq(&self, other: &Self) -> bool {
        self.directory_snapshots == other.directory_snapshots
            && self.file_snapshots == other.file_snapshots
            && self.git_snapshots == other.git_snapshots
            && self.min_poll_interval == other.min_poll_interval
    }
}

impl Eq for FilesystemWatcher {}

impl Default for FilesystemWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl FilesystemWatcher {
    /// Creates a new filesystem watcher with default 100ms throttle interval.
    pub fn new() -> Self {
        Self {
            directory_snapshots: HashMap::new(),
            file_snapshots: HashMap::new(),
            git_snapshots: HashMap::new(),
            last_poll: None,
            min_poll_interval: Duration::from_millis(100),
        }
    }

    /// Sets the minimum interval between actual filesystem stat checks.
    pub fn set_min_poll_interval(&mut self, interval: Duration) {
        self.min_poll_interval = interval;
    }

    /// Clears all stored snapshots.
    pub fn clear(&mut self) {
        self.directory_snapshots.clear();
        self.file_snapshots.clear();
        self.git_snapshots.clear();
        self.last_poll = None;
    }

    /// Takes initial baseline snapshots for the given directories and optional preview file.
    pub fn watch(&mut self, directories: &[PathBuf], preview_file: Option<&Path>) {
        for dir in directories {
            if !self.directory_snapshots.contains_key(dir) {
                let snap = capture_directory_snapshot(dir);
                self.directory_snapshots.insert(dir.clone(), snap);
            }
            if let Some(git_dir) = find_git_dir(dir)
                && !self.git_snapshots.contains_key(&git_dir)
            {
                let mtime = capture_git_mtime(&git_dir);
                self.git_snapshots.insert(git_dir, mtime);
            }
        }

        if let Some(file) = preview_file {
            let path_buf = file.to_path_buf();
            self.file_snapshots
                .entry(path_buf)
                .or_insert_with(|| capture_file_snapshot(file));
        }
    }

    /// Polls the filesystem for changes across the given directories and preview file.
    ///
    /// If less than `min_poll_interval` has elapsed since the last poll, returns an empty list.
    pub fn poll_changes(
        &mut self,
        directories: &[PathBuf],
        preview_file: Option<&Path>,
    ) -> Vec<FilesystemChange> {
        if let Some(last) = self.last_poll
            && last.elapsed() < self.min_poll_interval
        {
            return Vec::new();
        }
        self.last_poll = Some(Instant::now());
        self.check_changes_now(directories, preview_file)
    }

    /// Unconditionally checks for filesystem changes immediately without rate-limiting.
    pub fn check_changes_now(
        &mut self,
        directories: &[PathBuf],
        preview_file: Option<&Path>,
    ) -> Vec<FilesystemChange> {
        let mut changes = Vec::new();

        // 1. Check watched directories
        for dir in directories {
            let current = capture_directory_snapshot(dir);
            if let Some(previous) = self.directory_snapshots.get(dir) {
                if previous.exists && !current.exists {
                    changes.push(FilesystemChange::DirectoryDeleted(dir.clone()));
                } else if current.exists
                    && (previous.mtime != current.mtime
                        || previous.entry_count != current.entry_count)
                {
                    changes.push(FilesystemChange::DirectoryModified(dir.clone()));
                }
            }
            self.directory_snapshots.insert(dir.clone(), current);

            // Check Git repository if applicable
            if let Some(git_dir) = find_git_dir(dir) {
                let current_git = capture_git_mtime(&git_dir);
                if let Some(previous_git) = self.git_snapshots.get(&git_dir)
                    && *previous_git != current_git
                {
                    changes.push(FilesystemChange::GitStateChanged(dir.clone()));
                }
                self.git_snapshots.insert(git_dir, current_git);
            }
        }

        // 2. Check preview file
        if let Some(file) = preview_file {
            let path_buf = file.to_path_buf();
            let current = capture_file_snapshot(file);
            if let Some(previous) = self.file_snapshots.get(&path_buf) {
                if previous.exists && !current.exists {
                    changes.push(FilesystemChange::PreviewFileDeleted(path_buf.clone()));
                } else if current.exists
                    && (previous.mtime != current.mtime || previous.len != current.len)
                {
                    changes.push(FilesystemChange::PreviewFileModified(path_buf.clone()));
                }
            }
            self.file_snapshots.insert(path_buf, current);
        }

        changes
    }
}

/// Captures a lightweight snapshot of a directory.
fn capture_directory_snapshot(path: &Path) -> DirectorySnapshot {
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => {
            let mtime = meta.modified().ok();
            let entry_count = fs::read_dir(path).map(|r| r.count()).unwrap_or(0);
            DirectorySnapshot {
                mtime,
                entry_count,
                exists: true,
            }
        }
        _ => DirectorySnapshot {
            mtime: None,
            entry_count: 0,
            exists: false,
        },
    }
}

/// Captures a lightweight snapshot of a file.
fn capture_file_snapshot(path: &Path) -> FileSnapshot {
    match fs::metadata(path) {
        Ok(meta) if meta.is_file() => FileSnapshot {
            mtime: meta.modified().ok(),
            len: meta.len(),
            exists: true,
        },
        _ => FileSnapshot {
            mtime: None,
            len: 0,
            exists: false,
        },
    }
}

/// Finds the `.git` directory for a given path, if inside a Git repository.
fn find_git_dir(path: &Path) -> Option<PathBuf> {
    let mut current = path;
    loop {
        let git_candidate = current.join(".git");
        if git_candidate.exists() {
            return Some(git_candidate);
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent,
            _ => break,
        }
    }
    None
}

/// Captures the latest modified time among `.git/HEAD` and `.git/index`.
fn capture_git_mtime(git_dir: &Path) -> Option<SystemTime> {
    let head = git_dir.join("HEAD");
    let index = git_dir.join("index");

    let head_mtime = fs::metadata(&head).and_then(|m| m.modified()).ok();
    let index_mtime = fs::metadata(&index).and_then(|m| m.modified()).ok();

    match (head_mtime, index_mtime) {
        (Some(h), Some(i)) => Some(h.max(i)),
        (Some(h), None) => Some(h),
        (None, Some(i)) => Some(i),
        (None, None) => fs::metadata(git_dir).and_then(|m| m.modified()).ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;
    use std::fs::File;
    use std::io::Write;
    use std::thread::sleep;

    #[test]
    fn test_watcher_detects_file_creation_and_deletion() {
        let temp = TempDir::new("watcher_create_delete");
        let dir = temp.path().to_path_buf();

        let mut watcher = FilesystemWatcher::new();
        watcher.watch(std::slice::from_ref(&dir), None);

        // No changes initially
        let changes = watcher.check_changes_now(std::slice::from_ref(&dir), None);
        assert!(changes.is_empty());

        // Create a file in the directory
        sleep(Duration::from_millis(10));
        let file_path = dir.join("test.txt");
        let mut f = File::create(&file_path).unwrap();
        writeln!(f, "hello world").unwrap();
        drop(f);

        let changes = watcher.check_changes_now(std::slice::from_ref(&dir), None);
        assert_eq!(
            changes,
            vec![FilesystemChange::DirectoryModified(dir.clone())]
        );

        // Delete the file
        sleep(Duration::from_millis(10));
        fs::remove_file(&file_path).unwrap();

        let changes = watcher.check_changes_now(std::slice::from_ref(&dir), None);
        assert_eq!(
            changes,
            vec![FilesystemChange::DirectoryModified(dir.clone())]
        );
    }

    #[test]
    fn test_watcher_detects_preview_file_modification() {
        let temp = TempDir::new("watcher_preview_mod");
        let file_path = temp.path().join("preview.txt");
        let mut f = File::create(&file_path).unwrap();
        write!(f, "initial").unwrap();
        drop(f);

        let mut watcher = FilesystemWatcher::new();
        watcher.watch(&[], Some(&file_path));

        // Modify file content
        sleep(Duration::from_millis(15));
        let mut f = File::create(&file_path).unwrap();
        write!(f, "updated longer content").unwrap();
        drop(f);

        let changes = watcher.check_changes_now(&[], Some(&file_path));
        assert_eq!(
            changes,
            vec![FilesystemChange::PreviewFileModified(file_path.clone())]
        );
    }
}
