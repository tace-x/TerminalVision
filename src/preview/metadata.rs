//! Filesystem entry metadata preview domain logic.
//!
//! Provides structured, read-only metadata representation for regular files,
//! directories, and symbolic links without executing files, following symlinks
//! unsafely, or recursively traversing directory trees.
//!
//! Independent of Ratatui and Crossterm rendering layers.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::filesystem::entry::EntryKind;
use crate::filesystem::metadata::{LinkTarget, read_metadata};

/// Converts total seconds since UNIX epoch into UTC `(year, month, day, hour, min, sec)`.
fn secs_to_utc_datetime(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let sec = (secs % 60) as u32;
    let mins = secs / 60;
    let min = (mins % 60) as u32;
    let hours = mins / 60;
    let hour = (hours % 24) as u32;
    let days = (hours / 24) as u32;

    let (year, month, day) = days_to_ymd(days);
    (year, month, day, hour, min, sec)
}

fn days_to_ymd(days_since_epoch: u32) -> (u32, u32, u32) {
    let mut days = days_since_epoch;
    let mut year = 1970;

    loop {
        let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if is_leap { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let days_in_months = [
        31,
        if is_leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];

    let mut month = 1;
    for &dim in &days_in_months {
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }

    let day = days + 1;
    (year, month, day)
}

/// Formats a [`SystemTime`] into a readable UTC date-time string `YYYY-MM-DD HH:MM:SS UTC`.
pub fn format_system_time(time: SystemTime) -> String {
    let duration = match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => return "Before UNIX epoch".to_string(),
    };

    let secs = duration.as_secs();
    let (year, month, day, hour, min, sec) = secs_to_utc_datetime(secs);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{min:02}:{sec:02} UTC")
}

/// Formats a size in bytes to a human-readable display string (e.g., `1,024 B (1.0 KiB)`).
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        let kib = bytes as f64 / 1024.0;
        format!("{bytes} B ({kib:.1} KiB)")
    } else if bytes < 1024 * 1024 * 1024 {
        let mib = bytes as f64 / (1024.0 * 1024.0);
        format!("{bytes} B ({mib:.1} MiB)")
    } else {
        let gib = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        format!("{bytes} B ({gib:.2} GiB)")
    }
}

/// The prepared metadata preview of a filesystem entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataPreview {
    name: String,
    path: PathBuf,
    kind: EntryKind,
    size: Option<u64>,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    accessed: Option<SystemTime>,
    extension: Option<String>,
    child_count: Option<usize>,
    target_path: Option<PathBuf>,
    target_kind: Option<EntryKind>,
    target_size: Option<u64>,
    is_broken_symlink: bool,
    git_status: Option<String>,
    project_context: Option<String>,
}

impl MetadataPreview {
    /// Creates a new metadata preview with explicitly provided components.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        path: PathBuf,
        kind: EntryKind,
        size: Option<u64>,
        modified: Option<SystemTime>,
        created: Option<SystemTime>,
        accessed: Option<SystemTime>,
        extension: Option<String>,
        child_count: Option<usize>,
        target_path: Option<PathBuf>,
        target_kind: Option<EntryKind>,
        target_size: Option<u64>,
        is_broken_symlink: bool,
    ) -> Self {
        Self {
            name,
            path,
            kind,
            size,
            modified,
            created,
            accessed,
            extension,
            child_count,
            target_path,
            target_kind,
            target_size,
            is_broken_symlink,
            git_status: None,
            project_context: None,
        }
    }

    /// Loads and prepares metadata preview for the entry at `path`.
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_string());

        // Check if the path is a symlink directly first using symlink_metadata
        let sym_meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        let is_symlink = sym_meta.file_type().is_symlink();

        if is_symlink {
            let target_path = fs::read_link(path).ok();
            let link_size = Some(sym_meta.len());
            let modified = sym_meta.modified().ok();
            let created = sym_meta.created().ok();
            let accessed = sym_meta.accessed().ok();

            // Try reading target metadata
            match fs::metadata(path) {
                Ok(target_meta) => {
                    let target_kind = Some(EntryKind::from_file_type(target_meta.file_type()));
                    let target_size = if target_meta.is_dir() {
                        None
                    } else {
                        Some(target_meta.len())
                    };

                    Ok(Self {
                        name,
                        path: path.to_path_buf(),
                        kind: EntryKind::Symlink,
                        size: link_size,
                        modified,
                        created,
                        accessed,
                        extension,
                        child_count: None,
                        target_path,
                        target_kind,
                        target_size,
                        is_broken_symlink: false,
                        git_status: None,
                        project_context: None,
                    })
                }
                Err(_) => {
                    // Broken symlink
                    Ok(Self {
                        name,
                        path: path.to_path_buf(),
                        kind: EntryKind::Symlink,
                        size: link_size,
                        modified,
                        created,
                        accessed,
                        extension,
                        child_count: None,
                        target_path,
                        target_kind: None,
                        target_size: None,
                        is_broken_symlink: true,
                        git_status: None,
                        project_context: None,
                    })
                }
            }
        } else {
            let metadata = read_metadata(path).map_err(|e| e.to_string())?;
            let kind = metadata.kind();
            let size = metadata.size();
            let modified = metadata.modified();
            let created = metadata.created();
            let accessed = metadata.accessed();

            let child_count = if kind == EntryKind::Directory {
                fs::read_dir(path).map(|entries| entries.count()).ok()
            } else {
                None
            };

            let (target_path, target_kind, target_size, is_broken_symlink) = match metadata.target()
            {
                LinkTarget::Resolved(tm) => {
                    (fs::read_link(path).ok(), Some(tm.kind()), tm.size(), false)
                }
                LinkTarget::Unavailable { .. } => (fs::read_link(path).ok(), None, None, true),
                LinkTarget::NotApplicable => (None, None, None, false),
            };

            Ok(Self {
                name,
                path: path.to_path_buf(),
                kind,
                size,
                modified,
                created,
                accessed,
                extension,
                child_count,
                target_path,
                target_kind,
                target_size,
                is_broken_symlink,
                git_status: None,
                project_context: None,
            })
        }
    }

    /// The name of the entry.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The full path of the entry.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The kind of entry (File, Directory, Symlink, Other).
    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    /// Human-friendly display string for entry kind.
    pub fn kind_display(&self) -> &str {
        if self.is_broken_symlink {
            "Broken Symbolic Link"
        } else {
            match self.kind {
                EntryKind::File => "Regular File",
                EntryKind::Directory => "Directory",
                EntryKind::Symlink => "Symbolic Link",
                EntryKind::Other => "Special / Other",
            }
        }
    }

    /// Short badge string for entry kind.
    pub fn kind_badge(&self) -> &str {
        if self.is_broken_symlink {
            "[BROKEN LINK]"
        } else {
            match self.kind {
                EntryKind::File => "[FILE]",
                EntryKind::Directory => "[DIR]",
                EntryKind::Symlink => "[LINK]",
                EntryKind::Other => "[OTHER]",
            }
        }
    }

    /// The size in bytes, or None for directories.
    pub fn size(&self) -> Option<u64> {
        self.size
    }

    /// The formatted size string.
    pub fn size_display(&self) -> String {
        match self.size {
            Some(bytes) => format_size(bytes),
            None => "—".to_string(),
        }
    }

    /// Last modification time.
    pub fn modified(&self) -> Option<SystemTime> {
        self.modified
    }

    /// Formatted modification time.
    pub fn modified_display(&self) -> String {
        self.modified
            .map(format_system_time)
            .unwrap_or_else(|| "Unavailable".to_string())
    }

    /// Creation time when available.
    pub fn created(&self) -> Option<SystemTime> {
        self.created
    }

    /// Formatted creation time.
    pub fn created_display(&self) -> String {
        self.created
            .map(format_system_time)
            .unwrap_or_else(|| "Unavailable".to_string())
    }

    /// Last access time when available.
    pub fn accessed(&self) -> Option<SystemTime> {
        self.accessed
    }

    /// Formatted access time.
    pub fn accessed_display(&self) -> String {
        self.accessed
            .map(format_system_time)
            .unwrap_or_else(|| "Unavailable".to_string())
    }

    /// File extension if present.
    pub fn extension(&self) -> Option<&str> {
        self.extension.as_deref()
    }

    /// Immediate child count for directories (non-recursive).
    pub fn child_count(&self) -> Option<usize> {
        self.child_count
    }

    /// Formatted child count for directories.
    pub fn child_count_display(&self) -> String {
        match self.child_count {
            Some(1) => "1 item".to_string(),
            Some(count) => format!("{count} items"),
            None => "Unavailable".to_string(),
        }
    }

    /// Symlink target path.
    pub fn target_path(&self) -> Option<&Path> {
        self.target_path.as_deref()
    }

    /// Symlink target kind when resolved.
    pub fn target_kind(&self) -> Option<EntryKind> {
        self.target_kind
    }

    /// Symlink target size when resolved.
    pub fn target_size(&self) -> Option<u64> {
        self.target_size
    }

    /// Whether this is a broken symlink.
    pub fn is_broken_symlink(&self) -> bool {
        self.is_broken_symlink
    }

    /// Sets the Git status string for this entry.
    pub fn with_git_status(mut self, status: Option<String>) -> Self {
        self.git_status = status;
        self
    }

    /// Sets the project context string for this entry.
    pub fn with_project_context(mut self, context: Option<String>) -> Self {
        self.project_context = context;
        self
    }

    /// Returns the Git status description if available.
    pub fn git_status(&self) -> Option<&str> {
        self.git_status.as_deref()
    }

    /// Returns the project context description if available.
    pub fn project_context(&self) -> Option<&str> {
        self.project_context.as_deref()
    }
}

/// Helper function to load metadata preview safely.
pub fn load_metadata_preview(path: &Path) -> Result<MetadataPreview, String> {
    MetadataPreview::from_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;
    use std::fs;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn test_format_size_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1024 B (1.0 KiB)");
        assert_eq!(format_size(2048), "2048 B (2.0 KiB)");
        assert_eq!(format_size(1024 * 1024), "1048576 B (1.0 MiB)");
        assert_eq!(format_size(1024 * 1024 * 1024), "1073741824 B (1.00 GiB)");
    }

    #[test]
    fn test_format_system_time_epoch() {
        let epoch = UNIX_EPOCH;
        assert_eq!(format_system_time(epoch), "1970-01-01 00:00:00 UTC");

        let next_day = UNIX_EPOCH + Duration::from_secs(86400);
        assert_eq!(format_system_time(next_day), "1970-01-02 00:00:00 UTC");
    }

    #[test]
    fn test_file_metadata_preview() {
        let temp = TempDir::new("meta-file");
        let file = temp.path().join("document.pdf");
        fs::write(&file, b"sample pdf content data").unwrap();

        let meta = MetadataPreview::from_path(&file).expect("metadata should load");
        assert_eq!(meta.name(), "document.pdf");
        assert_eq!(meta.kind(), EntryKind::File);
        assert_eq!(meta.extension(), Some("pdf"));
        assert_eq!(meta.size(), Some(23));
        assert!(meta.modified().is_some());
        assert!(!meta.is_broken_symlink());
        assert_eq!(meta.kind_badge(), "[FILE]");
    }

    #[test]
    fn test_directory_metadata_preview() {
        let temp = TempDir::new("meta-dir");
        let dir = temp.path().join("my_folder");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("a.txt"), b"1").unwrap();
        fs::write(dir.join("b.txt"), b"2").unwrap();

        let meta = MetadataPreview::from_path(&dir).expect("metadata should load");
        assert_eq!(meta.name(), "my_folder");
        assert_eq!(meta.kind(), EntryKind::Directory);
        assert_eq!(meta.extension(), None);
        assert_eq!(meta.size(), None);
        assert_eq!(meta.child_count(), Some(2));
        assert_eq!(meta.child_count_display(), "2 items");
        assert_eq!(meta.kind_badge(), "[DIR]");
    }

    #[test]
    #[cfg(unix)]
    fn test_symlink_metadata_preview() {
        use std::os::unix::fs::symlink;
        let temp = TempDir::new("meta-sym");
        let target = temp.path().join("target.txt");
        fs::write(&target, b"target file content").unwrap();

        let link = temp.path().join("link.txt");
        symlink(&target, &link).unwrap();

        let meta = MetadataPreview::from_path(&link).expect("metadata should load");
        assert_eq!(meta.name(), "link.txt");
        assert_eq!(meta.kind(), EntryKind::Symlink);
        assert_eq!(meta.kind_badge(), "[LINK]");
        assert!(!meta.is_broken_symlink());
        assert_eq!(meta.target_kind(), Some(EntryKind::File));
        assert_eq!(meta.target_size(), Some(19));
    }

    #[test]
    #[cfg(unix)]
    fn test_broken_symlink_metadata_preview() {
        use std::os::unix::fs::symlink;
        let temp = TempDir::new("meta-broken-sym");
        let link = temp.path().join("dangling_link");
        symlink("nonexistent_target_path", &link).unwrap();

        let meta = MetadataPreview::from_path(&link).expect("metadata should load");
        assert_eq!(meta.name(), "dangling_link");
        assert_eq!(meta.kind(), EntryKind::Symlink);
        assert!(meta.is_broken_symlink());
        assert_eq!(meta.kind_badge(), "[BROKEN LINK]");
        assert_eq!(meta.target_kind(), None);
    }

    #[test]
    fn test_unicode_metadata_preview() {
        let temp = TempDir::new("meta-unicode");
        let file = temp.path().join("🦀_日本語_test.txt");
        fs::write(&file, b"unicode bytes").unwrap();

        let meta = MetadataPreview::from_path(&file).expect("metadata should load");
        assert_eq!(meta.name(), "🦀_日本語_test.txt");
        assert_eq!(meta.kind(), EntryKind::File);
        assert_eq!(meta.extension(), Some("txt"));
    }

    #[test]
    fn test_missing_file_metadata_preview() {
        let missing = Path::new("/path/that/does/not/exist/missing_file.txt");
        let result = MetadataPreview::from_path(missing);
        assert!(result.is_err());
    }
}
