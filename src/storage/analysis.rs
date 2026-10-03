//! Storage analysis data models and unit formatting.
//!
//! Organizes aggregated directory storage statistics by category, direct children,
//! and largest files, formatted using standard human-readable storage units.

use std::path::PathBuf;

use crate::filesystem::classification::FileCategory;

/// Formats byte counts using standard decimal units (B, KB, MB, GB, TB).
pub fn format_storage_size(bytes: u64) -> String {
    const KB: u64 = 1_000;
    const MB: u64 = 1_000 * KB;
    const GB: u64 = 1_000 * MB;
    const TB: u64 = 1_000 * GB;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// A direct child item (subfolder or file) analyzed inside the storage scope.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageChildItem {
    /// Item name.
    pub name: String,
    /// Absolute path.
    pub path: PathBuf,
    /// Whether this item is a directory.
    pub is_dir: bool,
    /// Aggregate byte size.
    pub bytes: u64,
    /// Percentage share of total scanned scope (0.0 to 100.0).
    pub percentage: f32,
    /// Number of files contained within if a directory.
    pub file_count: usize,
}

/// A large individual file detected during the storage analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageFileItem {
    /// File name.
    pub name: String,
    /// Absolute path.
    pub path: PathBuf,
    /// Byte size.
    pub bytes: u64,
    /// Detected file category.
    pub category: FileCategory,
}

/// Comprehensive analysis of a storage scope.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageAnalysis {
    /// The root path of the analyzed storage scope.
    pub scope_path: PathBuf,
    /// Total aggregate scanned bytes.
    pub total_bytes: u64,
    /// Total number of scanned regular files.
    pub total_files: usize,
    /// Total number of scanned directories.
    pub total_dirs: usize,
    /// Number of directories skipped due to permissions or I/O errors.
    pub skipped_permissions: usize,
    /// Breakdown by file category: (Category, Total Bytes, File Count), sorted by bytes descending.
    pub category_breakdown: Vec<(FileCategory, u64, usize)>,
    /// Direct child folders/files with individual sizes and percentage shares.
    pub direct_children: Vec<StorageChildItem>,
    /// Top largest files sorted by size descending.
    pub largest_files: Vec<StorageFileItem>,
    /// Whether the background scan has finished.
    pub is_complete: bool,
}

impl StorageAnalysis {
    /// Creates a new empty storage analysis for `scope_path`.
    pub fn new(scope_path: PathBuf) -> Self {
        Self {
            scope_path,
            total_bytes: 0,
            total_files: 0,
            total_dirs: 0,
            skipped_permissions: 0,
            category_breakdown: Vec::new(),
            direct_children: Vec::new(),
            largest_files: Vec::new(),
            is_complete: false,
        }
    }

    /// Formatted total size string.
    pub fn formatted_total_size(&self) -> String {
        format_storage_size(self.total_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_storage_size_units() {
        assert_eq!(format_storage_size(500), "500 B");
        assert_eq!(format_storage_size(1_500), "1.5 KB");
        assert_eq!(format_storage_size(10_400_000), "10.4 MB");
        assert_eq!(format_storage_size(42_800_000_000), "42.8 GB");
        assert_eq!(format_storage_size(2_500_000_000_000), "2.5 TB");
    }
}
