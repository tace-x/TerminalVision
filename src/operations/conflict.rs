//! Conflict resolution for filesystem operations.
//!
//! When copying or moving an entry to an existing destination, provides interactive
//! conflict choices (Replace, Skip, Rename, Replace All, Skip All, Cancel) without
//! silently destroying user data.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// User choice for resolving a destination collision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Overwrite the existing file with the new one.
    Replace,
    /// Skip copying/moving this file and continue.
    Skip,
    /// Keep both files by automatically renaming the destination.
    Rename,
    /// Overwrite this and all subsequent conflicting files.
    ReplaceAll,
    /// Skip this and all subsequent conflicting files.
    SkipAll,
    /// Cancel the entire operation immediately.
    Cancel,
}

impl ConflictResolution {
    /// List of options for standard single-conflict prompt.
    pub const ALL_OPTIONS: [ConflictResolution; 6] = [
        ConflictResolution::Replace,
        ConflictResolution::Skip,
        ConflictResolution::Rename,
        ConflictResolution::ReplaceAll,
        ConflictResolution::SkipAll,
        ConflictResolution::Cancel,
    ];

    /// User-visible button label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Replace => "Replace",
            Self::Skip => "Skip",
            Self::Rename => "Rename",
            Self::ReplaceAll => "Replace All",
            Self::SkipAll => "Skip All",
            Self::Cancel => "Cancel",
        }
    }
}

/// Information presenting a conflict to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictPrompt {
    /// Source file path being copied or moved.
    pub source_path: PathBuf,
    /// Existing file path at destination.
    pub destination_path: PathBuf,
    /// Size of source file in bytes.
    pub source_size: u64,
    /// Size of destination file in bytes.
    pub destination_size: u64,
    /// Source file last modification time.
    pub source_modified: Option<SystemTime>,
    /// Destination file last modification time.
    pub destination_modified: Option<SystemTime>,
    /// Suggested unique rename target.
    pub suggested_rename: PathBuf,
    /// Currently selected option index in dialog.
    pub selected_option_index: usize,
}

impl ConflictPrompt {
    /// Creates a conflict prompt comparing `source` and existing `destination`.
    pub fn new(source: &Path, destination: &Path) -> Self {
        let source_meta = fs::metadata(source).ok();
        let dest_meta = fs::metadata(destination).ok();

        let source_size = source_meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let destination_size = dest_meta.as_ref().map(|m| m.len()).unwrap_or(0);

        let source_modified = source_meta.and_then(|m| m.modified().ok());
        let destination_modified = dest_meta.and_then(|m| m.modified().ok());

        let suggested_rename = generate_unique_rename(destination);

        Self {
            source_path: source.to_path_buf(),
            destination_path: destination.to_path_buf(),
            source_size,
            destination_size,
            source_modified,
            destination_modified,
            suggested_rename,
            selected_option_index: 0,
        }
    }

    /// Moves selection to next option.
    pub fn next_option(&mut self) {
        self.selected_option_index =
            (self.selected_option_index + 1) % ConflictResolution::ALL_OPTIONS.len();
    }

    /// Moves selection to previous option.
    pub fn prev_option(&mut self) {
        if self.selected_option_index == 0 {
            self.selected_option_index = ConflictResolution::ALL_OPTIONS.len() - 1;
        } else {
            self.selected_option_index -= 1;
        }
    }

    /// Returns the currently highlighted option.
    pub fn current_resolution(&self) -> ConflictResolution {
        ConflictResolution::ALL_OPTIONS[self.selected_option_index]
    }
}

/// Generates a non-conflicting destination path (e.g. `file (1).txt`).
pub fn generate_unique_rename(dest: &Path) -> PathBuf {
    let parent = dest.parent().unwrap_or_else(|| Path::new(""));
    let stem = dest.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = dest
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();

    for index in 1..10000 {
        let candidate_name = format!("{stem} ({index}){ext}");
        let candidate_path = parent.join(candidate_name);
        if !candidate_path.exists() {
            return candidate_path;
        }
    }

    parent.join(format!("{stem} (copy){ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;

    #[test]
    fn test_unique_rename_generation() {
        let temp = TempDir::new("conflict-test");
        let orig = temp.path().join("document.txt");
        fs::write(&orig, b"existing").unwrap();

        let unique = generate_unique_rename(&orig);
        assert_eq!(
            unique.file_name().and_then(|s| s.to_str()).unwrap(),
            "document (1).txt"
        );
    }
}
