//! Fast, non-blocking summary preview for directories.
//!
//! Gathers immediate directory statistics (file count, directory count, immediate size,
//! and Git status) safely without recursive filesystem walking.

use std::fs;
use std::path::{Path, PathBuf};

/// Structured directory preview information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryPreview {
    /// Directory name.
    pub name: String,
    /// Full path.
    pub path: PathBuf,
    /// Total direct items count.
    pub item_count: usize,
    /// Direct regular files count.
    pub file_count: usize,
    /// Direct subdirectories count.
    pub dir_count: usize,
    /// Symlinks count.
    pub symlink_count: usize,
    /// Aggregate byte size of immediate files.
    pub immediate_size: u64,
    /// Git repository summary if inside a repository.
    pub git_status: Option<String>,
    /// Sample list of contained entries (first few).
    pub sample_entries: Vec<String>,
}

impl DirectoryPreview {
    /// Inspects the directory at `path` non-recursively.
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("/")
            .to_string();

        let read_dir = fs::read_dir(path).map_err(|e| e.to_string())?;

        let mut item_count = 0;
        let mut file_count = 0;
        let mut dir_count = 0;
        let mut symlink_count = 0;
        let mut immediate_size = 0u64;
        let mut sample_entries = Vec::new();

        for entry in read_dir.flatten() {
            item_count += 1;
            let file_name = entry.file_name().to_string_lossy().to_string();

            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    dir_count += 1;
                    if sample_entries.len() < 8 {
                        sample_entries.push(format!("📁 {file_name}/"));
                    }
                } else if file_type.is_symlink() {
                    symlink_count += 1;
                    if sample_entries.len() < 8 {
                        sample_entries.push(format!("🔗 {file_name}"));
                    }
                } else {
                    file_count += 1;
                    if let Ok(meta) = entry.metadata() {
                        immediate_size = immediate_size.saturating_add(meta.len());
                    }
                    if sample_entries.len() < 8 {
                        sample_entries.push(format!("📄 {file_name}"));
                    }
                }
            }
        }

        // Check Git status if .git directory exists nearby
        let git_status = if path.join(".git").exists() {
            Some("Git Repository Root".to_string())
        } else {
            None
        };

        Ok(Self {
            name,
            path: path.to_path_buf(),
            item_count,
            file_count,
            dir_count,
            symlink_count,
            immediate_size,
            git_status,
            sample_entries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;

    #[test]
    fn test_directory_preview_summary() {
        let temp = TempDir::new("dir-preview-test");
        let dir = temp.path();

        fs::write(dir.join("file1.txt"), b"12345").unwrap();
        fs::write(dir.join("file2.txt"), b"67890").unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();

        let preview = DirectoryPreview::from_path(dir).expect("directory preview should succeed");
        assert_eq!(preview.item_count, 3);
        assert_eq!(preview.file_count, 2);
        assert_eq!(preview.dir_count, 1);
        assert_eq!(preview.immediate_size, 10);
        assert_eq!(preview.sample_entries.len(), 3);
    }
}
