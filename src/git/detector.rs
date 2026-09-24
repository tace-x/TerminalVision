//! Read-only Git repository and worktree detection.

use std::fs;
use std::path::{Path, PathBuf};

/// The detected Git repository state for a directory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GitRepository {
    /// Not in a Git repository or directory is missing/invalid.
    #[default]
    None,
    /// In a Git repository.
    Detected {
        /// The root directory of the Git repository.
        root: PathBuf,
        /// Whether `.git` at root is a file (worktree/submodule) rather than a directory.
        is_worktree: bool,
    },
}

impl GitRepository {
    /// Whether a Git repository was detected.
    pub fn is_repo(&self) -> bool {
        matches!(self, Self::Detected { .. })
    }

    /// The repository root directory path if detected.
    pub fn root(&self) -> Option<&Path> {
        match self {
            Self::Detected { root, .. } => Some(root.as_path()),
            Self::None => None,
        }
    }

    /// The display name of the repository.
    pub fn repo_name(&self) -> Option<&str> {
        let root = self.root()?;
        root.file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .or_else(|| root.to_str())
    }

    /// Whether the repository is a `.git` worktree file.
    pub fn is_worktree(&self) -> bool {
        match self {
            Self::Detected { is_worktree, .. } => *is_worktree,
            Self::None => false,
        }
    }
}

/// Detects whether `path` belongs to a Git repository by walking parent directories up to the filesystem root.
pub fn detect_repository(path: &Path) -> GitRepository {
    if path.as_os_str().is_empty() || !path.exists() {
        return GitRepository::None;
    }

    let mut current = if path.is_dir() {
        path.to_path_buf()
    } else if let Some(parent) = path.parent() {
        parent.to_path_buf()
    } else {
        return GitRepository::None;
    };

    loop {
        let git_entry = current.join(".git");
        if let Ok(meta) = fs::metadata(&git_entry) {
            if meta.is_dir() {
                return GitRepository::Detected {
                    root: current,
                    is_worktree: false,
                };
            } else if meta.is_file() {
                return GitRepository::Detected {
                    root: current,
                    is_worktree: true,
                };
            }
        }

        if !current.pop() {
            break;
        }
    }

    GitRepository::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;

    #[test]
    fn test_git_detection_directory_inside_repository() {
        let temp_dir = TempDir::new("git-repo-test");
        let repo_root = temp_dir.path().to_path_buf();
        let sub_dir = repo_root.join("src").join("ui");

        fs::create_dir_all(&sub_dir).expect("create sub_dir");
        fs::create_dir(repo_root.join(".git")).expect("create .git dir");

        let repo = detect_repository(&sub_dir);
        assert!(repo.is_repo());
        assert_eq!(repo.root(), Some(repo_root.as_path()));
        assert!(!repo.is_worktree());
        assert_eq!(
            repo.repo_name(),
            repo_root.file_name().and_then(|n| n.to_str())
        );
    }

    #[test]
    fn test_git_detection_at_repository_root() {
        let temp_dir = TempDir::new("git-root-test");
        let repo_root = temp_dir.path().to_path_buf();
        fs::create_dir(repo_root.join(".git")).expect("create .git dir");

        let repo = detect_repository(&repo_root);
        assert!(repo.is_repo());
        assert_eq!(repo.root(), Some(repo_root.as_path()));
        assert!(!repo.is_worktree());
    }

    #[test]
    fn test_git_detection_worktree_file() {
        let temp_dir = TempDir::new("git-worktree-test");
        let repo_root = temp_dir.path().to_path_buf();
        let git_file = repo_root.join(".git");
        fs::write(&git_file, "gitdir: /path/to/gitdir").expect("write .git file");

        let sub_dir = repo_root.join("subfolder");
        fs::create_dir(&sub_dir).expect("create subfolder");

        let repo = detect_repository(&sub_dir);
        assert!(repo.is_repo());
        assert_eq!(repo.root(), Some(repo_root.as_path()));
        assert!(repo.is_worktree());
    }

    #[test]
    fn test_git_detection_no_repository() {
        let temp_dir = TempDir::new("no-git-test");
        let plain_dir = temp_dir.path().to_path_buf();

        let repo = detect_repository(&plain_dir);
        assert!(!repo.is_repo());
        assert_eq!(repo.root(), None);
        assert_eq!(repo.repo_name(), None);
    }

    #[test]
    fn test_git_detection_missing_or_empty_path() {
        let missing = PathBuf::from("/path/that/does/not/exist/12345");
        assert!(!detect_repository(&missing).is_repo());

        let empty = PathBuf::from("");
        assert!(!detect_repository(&empty).is_repo());
    }

    #[test]
    fn test_git_detection_unicode_and_spaces_in_paths() {
        let temp_dir = TempDir::new("git-unicode-spaces");
        let repo_root = temp_dir.path().join("📁 Project 🦀 with spaces");
        let sub_dir = repo_root.join("日本語 subfolder");
        fs::create_dir_all(&sub_dir).expect("create sub_dir");
        fs::create_dir(repo_root.join(".git")).expect("create .git dir");

        let repo = detect_repository(&sub_dir);
        assert!(repo.is_repo());
        assert_eq!(repo.root(), Some(repo_root.as_path()));
        assert_eq!(repo.repo_name(), Some("📁 Project 🦀 with spaces"));
    }
}
