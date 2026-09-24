//! Cross-platform filesystem path utilities.
//!
//! Provides home directory expansion, path normalization, and root discovery
//! without executing shell commands or relying on external process invocations.

use std::path::{Component, Path, PathBuf};

/// Returns the current user's home directory if determinable from the environment.
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| {
                let drive = std::env::var_os("HOMEDRIVE")?;
                let path = std::env::var_os("HOMEPATH")?;
                let mut full = PathBuf::from(drive);
                full.push(path);
                Some(full)
            })
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// Expands a leading tilde (`~` or `~/...` or `~\...`) in a path string into the user's home directory.
pub fn expand_home(path_str: &str) -> PathBuf {
    let trimmed = path_str.trim();
    if trimmed == "~" {
        if let Some(home) = home_dir() {
            return home;
        }
        return PathBuf::from(trimmed);
    }

    if let Some(stripped) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
        && let Some(mut home) = home_dir()
    {
        home.push(stripped);
        return home;
    }

    PathBuf::from(trimmed)
}

/// Resolves an input path string against a base directory.
///
/// Supports:
/// - Absolute paths
/// - Home-relative paths (`~/...`)
/// - Relative paths (resolved against `base`)
/// - Windows drive paths (`C:\...`) and UNC paths
/// - Paths with spaces and Unicode characters
pub fn resolve_target_path(input: &str, base: &Path) -> PathBuf {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return base.to_path_buf();
    }

    // 1. Home directory expansion
    if trimmed == "~" || trimmed.starts_with("~/") || trimmed.starts_with("~\\") {
        return normalize_path(&expand_home(trimmed));
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        normalize_path(path)
    } else {
        normalize_path(&base.join(path))
    }
}

/// Computes the filesystem root for a given path.
///
/// On Unix, returns `/`. On Windows, returns the drive root (e.g. `C:\`).
pub fn filesystem_root(path: &Path) -> PathBuf {
    for component in path.components() {
        if let Component::Prefix(prefix) = component {
            let mut root = PathBuf::from(prefix.as_os_str());
            root.push("\\");
            return root;
        } else if let Component::RootDir = component {
            return PathBuf::from(Component::RootDir.as_os_str());
        }
    }

    #[cfg(windows)]
    {
        PathBuf::from("C:\\")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/")
    }
}

/// Lexically normalizes `..` and `.` components in a path without hitting disk.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Only pop if previous component is not RootDir or Prefix
                if let Some(Component::Normal(_)) = normalized.components().next_back() {
                    normalized.pop();
                } else if !normalized.is_absolute() && normalized.as_os_str().is_empty() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_home_tilde() {
        let home = home_dir();
        if let Some(h) = home {
            assert_eq!(expand_home("~"), h);
            assert_eq!(expand_home("~/documents"), h.join("documents"));
            assert_eq!(expand_home("  ~/projects  "), h.join("projects"));
        }
    }

    #[test]
    fn test_resolve_target_path_relative_and_absolute() {
        let base = Path::new("/base/dir");
        assert_eq!(
            resolve_target_path("child", base),
            PathBuf::from("/base/dir/child")
        );
        assert_eq!(
            resolve_target_path("child/sub", base),
            PathBuf::from("/base/dir/child/sub")
        );
        assert_eq!(
            resolve_target_path("../sibling", base),
            PathBuf::from("/base/sibling")
        );
        assert_eq!(
            resolve_target_path("/var/log", base),
            PathBuf::from("/var/log")
        );
    }

    #[test]
    fn test_resolve_target_path_with_spaces_and_unicode() {
        let base = Path::new("/base/dir");
        assert_eq!(
            resolve_target_path("my folder/file 🦀.txt", base),
            PathBuf::from("/base/dir/my folder/file 🦀.txt")
        );
    }

    #[test]
    fn test_filesystem_root() {
        assert_eq!(filesystem_root(Path::new("/var/log")), PathBuf::from("/"));
        assert_eq!(filesystem_root(Path::new("/")), PathBuf::from("/"));
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path(Path::new("/a/b/../c")),
            PathBuf::from("/a/c")
        );
        assert_eq!(
            normalize_path(Path::new("/a/./b/./c")),
            PathBuf::from("/a/b/c")
        );
        assert_eq!(
            normalize_path(Path::new("/a/b/c/../../d")),
            PathBuf::from("/a/d")
        );
    }
}
