//! Asynchronous, non-blocking background scanner for storage analysis.
//!
//! Safely recurses through filesystem hierarchies with symlink protection,
//! permission error recovery, atomic live progress tracking, and cooperative cancellation.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use crate::filesystem::classification::{FileCategory, classify_file_by_extension_and_magic};
use crate::storage::analysis::{StorageAnalysis, StorageChildItem, StorageFileItem};

/// Limits the number of top largest files recorded.
const MAX_LARGEST_FILES: usize = 20;

/// Background storage scanner.
#[derive(Debug, Clone)]
pub struct StorageScanner {
    cancel_token: Arc<AtomicBool>,
    items_scanned: Arc<AtomicUsize>,
    bytes_scanned: Arc<AtomicU64>,
}

impl StorageScanner {
    /// Creates a new storage scanner with shared progress and cancellation handles.
    pub fn new(
        cancel_token: Arc<AtomicBool>,
        items_scanned: Arc<AtomicUsize>,
        bytes_scanned: Arc<AtomicU64>,
    ) -> Self {
        Self {
            cancel_token,
            items_scanned,
            bytes_scanned,
        }
    }

    /// Recursively scans `root_path` synchronously within the worker thread.
    pub fn scan_directory(&self, root_path: &Path) -> StorageAnalysis {
        let mut analysis = StorageAnalysis::new(root_path.to_path_buf());

        // 1. Enumerate direct children first to establish breakdown buckets
        let direct_entries = match fs::read_dir(root_path) {
            Ok(rd) => rd.flatten().collect::<Vec<_>>(),
            Err(_) => {
                analysis.skipped_permissions += 1;
                analysis.is_complete = true;
                return analysis;
            }
        };

        let mut child_sizes: HashMap<PathBuf, (String, bool, u64, usize)> = HashMap::new();
        for entry in &direct_entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            child_sizes.insert(path, (name, is_dir, 0u64, 0usize));
        }

        let mut category_totals: HashMap<FileCategory, (u64, usize)> = HashMap::new();
        let mut largest_files: Vec<StorageFileItem> = Vec::new();
        let mut visited_dirs: HashSet<PathBuf> = HashSet::new();

        // 2. Scan each direct child
        for entry in &direct_entries {
            if self.cancel_token.load(Ordering::Relaxed) {
                break;
            }

            let child_path = entry.path();
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => {
                    analysis.skipped_permissions += 1;
                    continue;
                }
            };

            if ft.is_dir() {
                analysis.total_dirs += 1;
                self.items_scanned.fetch_add(1, Ordering::Relaxed);

                let mut sub_bytes = 0u64;
                let mut sub_files = 0usize;

                self.scan_subtree(
                    &child_path,
                    &mut sub_bytes,
                    &mut sub_files,
                    &mut category_totals,
                    &mut largest_files,
                    &mut visited_dirs,
                    &mut analysis.skipped_permissions,
                    &mut analysis.total_dirs,
                );

                if let Some(entry) = child_sizes.get_mut(&child_path) {
                    entry.2 = sub_bytes;
                    entry.3 = sub_files;
                }

                analysis.total_bytes = analysis.total_bytes.saturating_add(sub_bytes);
                analysis.total_files = analysis.total_files.saturating_add(sub_files);
            } else if ft.is_file() {
                analysis.total_files += 1;
                self.items_scanned.fetch_add(1, Ordering::Relaxed);

                let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
                self.bytes_scanned.fetch_add(len, Ordering::Relaxed);
                analysis.total_bytes = analysis.total_bytes.saturating_add(len);

                if let Some(entry) = child_sizes.get_mut(&child_path) {
                    entry.2 = len;
                    entry.3 = 1;
                }

                let (cat, _) = classify_file_by_extension_and_magic(&child_path);
                let cat_entry = category_totals.entry(cat).or_insert((0, 0));
                cat_entry.0 = cat_entry.0.saturating_add(len);
                cat_entry.1 += 1;

                insert_largest_file(
                    &mut largest_files,
                    StorageFileItem {
                        name: entry.file_name().to_string_lossy().to_string(),
                        path: child_path,
                        bytes: len,
                        category: cat,
                    },
                );
            }
        }

        // 3. Compile direct children list with percentages
        let total_b = analysis.total_bytes.max(1) as f32;
        let mut direct_children = Vec::new();
        for (path, (name, is_dir, bytes, file_count)) in child_sizes {
            let percentage = ((bytes as f32) / total_b) * 100.0;
            direct_children.push(StorageChildItem {
                name,
                path,
                is_dir,
                bytes,
                percentage,
                file_count,
            });
        }
        direct_children.sort_by_key(|a| std::cmp::Reverse(a.bytes));
        analysis.direct_children = direct_children;

        // 4. Compile category breakdown
        let mut cat_breakdown: Vec<_> = category_totals
            .into_iter()
            .map(|(cat, (bytes, count))| (cat, bytes, count))
            .collect();
        cat_breakdown.sort_by_key(|a| std::cmp::Reverse(a.1));
        analysis.category_breakdown = cat_breakdown;

        analysis.largest_files = largest_files;
        analysis.is_complete = !self.cancel_token.load(Ordering::Relaxed);

        analysis
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_subtree(
        &self,
        dir_path: &Path,
        sub_bytes: &mut u64,
        sub_files: &mut usize,
        category_totals: &mut HashMap<FileCategory, (u64, usize)>,
        largest_files: &mut Vec<StorageFileItem>,
        visited_dirs: &mut HashSet<PathBuf>,
        skipped: &mut usize,
        total_dirs: &mut usize,
    ) {
        if self.cancel_token.load(Ordering::Relaxed) {
            return;
        }

        // Symlink safety: do not follow directory symlinks recursively to prevent loops
        if let Ok(sym_meta) = fs::symlink_metadata(dir_path)
            && sym_meta.file_type().is_symlink()
        {
            return;
        }

        if !visited_dirs.insert(dir_path.to_path_buf()) {
            return;
        }

        let read_dir = match fs::read_dir(dir_path) {
            Ok(rd) => rd,
            Err(_) => {
                *skipped += 1;
                return;
            }
        };

        for entry_res in read_dir {
            if self.cancel_token.load(Ordering::Relaxed) {
                return;
            }

            let entry = match entry_res {
                Ok(e) => e,
                Err(_) => {
                    *skipped += 1;
                    continue;
                }
            };

            let path = entry.path();
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => {
                    *skipped += 1;
                    continue;
                }
            };

            if ft.is_dir() {
                // Do not follow symlink directories
                if !ft.is_symlink() {
                    *total_dirs += 1;
                    self.items_scanned.fetch_add(1, Ordering::Relaxed);
                    self.scan_subtree(
                        &path,
                        sub_bytes,
                        sub_files,
                        category_totals,
                        largest_files,
                        visited_dirs,
                        skipped,
                        total_dirs,
                    );
                }
            } else if ft.is_file() {
                *sub_files += 1;
                self.items_scanned.fetch_add(1, Ordering::Relaxed);

                let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
                self.bytes_scanned.fetch_add(len, Ordering::Relaxed);
                *sub_bytes = sub_bytes.saturating_add(len);

                let (cat, _) = classify_file_by_extension_and_magic(&path);
                let cat_entry = category_totals.entry(cat).or_insert((0, 0));
                cat_entry.0 = cat_entry.0.saturating_add(len);
                cat_entry.1 += 1;

                insert_largest_file(
                    largest_files,
                    StorageFileItem {
                        name: entry.file_name().to_string_lossy().to_string(),
                        path,
                        bytes: len,
                        category: cat,
                    },
                );
            }
        }
    }
}

fn insert_largest_file(list: &mut Vec<StorageFileItem>, item: StorageFileItem) {
    if list.len() < MAX_LARGEST_FILES {
        list.push(item);
        list.sort_by_key(|a| std::cmp::Reverse(a.bytes));
    } else if item.bytes > list.last().map(|f| f.bytes).unwrap_or(0) {
        list.pop();
        list.push(item);
        list.sort_by_key(|a| std::cmp::Reverse(a.bytes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_storage_scanner_runs_and_aggregates() {
        let temp = TempDir::new().unwrap();
        let sub = temp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("file1.rs"), b"12345").unwrap();
        fs::write(temp.path().join("file2.png"), b"1234567890").unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let items = Arc::new(AtomicUsize::new(0));
        let bytes = Arc::new(AtomicU64::new(0));

        let scanner = StorageScanner::new(cancel, items, bytes);
        let analysis = scanner.scan_directory(temp.path());

        assert_eq!(analysis.total_files, 2);
        assert_eq!(analysis.total_bytes, 15);
        assert_eq!(analysis.direct_children.len(), 2);
        assert!(analysis.is_complete);
    }
}
