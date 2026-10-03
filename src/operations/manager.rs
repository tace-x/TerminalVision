//! Operation Manager and execution coordinator for smart filesystem tasks.
//!
//! Handles chunked copy/move execution, cooperative cancellation, safe pause/resume,
//! conflict resolution prompts, error recovery dialogues, and history logging.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::app::state::OperationKind;
use crate::operations::conflict::{ConflictPrompt, ConflictResolution};
use crate::operations::history::OperationHistory;
use crate::operations::progress::OperationMetrics;

/// Chunk size for copy/move operations (64 KiB) for smooth progress and cooperative interruption.
pub const OPERATION_CHUNK_SIZE: usize = 64 * 1024;

/// Error prompt for a failed file operation item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationErrorPrompt {
    /// The kind of operation.
    pub kind: OperationKind,
    /// The affected path.
    pub path: PathBuf,
    /// Concise error message.
    pub error_message: String,
    /// Selected recovery option index (0: Retry, 1: Skip, 2: Cancel).
    pub selected_option: usize,
}

impl OperationErrorPrompt {
    /// Creates a new error prompt.
    pub fn new(kind: OperationKind, path: &Path, error_message: String) -> Self {
        Self {
            kind,
            path: path.to_path_buf(),
            error_message,
            selected_option: 0,
        }
    }

    /// Moves selection to next recovery option.
    pub fn next_option(&mut self) {
        self.selected_option = (self.selected_option + 1) % 3;
    }

    /// Moves selection to previous recovery option.
    pub fn prev_option(&mut self) {
        if self.selected_option == 0 {
            self.selected_option = 2;
        } else {
            self.selected_option -= 1;
        }
    }
}

/// Central manager and coordinator for smart filesystem operations.
#[derive(Debug, Clone)]
pub struct OperationManager {
    /// Active metrics tracker for currently running operation if any.
    pub active_metrics: Option<OperationMetrics>,
    /// Cooperative cancellation flag.
    pub cancel_flag: Arc<AtomicBool>,
    /// Pause state flag.
    pub pause_flag: Arc<AtomicBool>,
    /// Active conflict resolution prompt if blocked by a destination collision.
    pub active_conflict: Option<ConflictPrompt>,
    /// Active error prompt if blocked by an operation error.
    pub active_error: Option<OperationErrorPrompt>,
    /// Remembered bulk conflict resolution (ReplaceAll / SkipAll).
    pub bulk_conflict_resolution: Option<ConflictResolution>,
    /// Operation history log.
    pub history: OperationHistory,
}

impl PartialEq for OperationManager {
    fn eq(&self, other: &Self) -> bool {
        self.active_metrics == other.active_metrics
            && self.active_conflict == other.active_conflict
            && self.active_error == other.active_error
            && self.bulk_conflict_resolution == other.bulk_conflict_resolution
            && self.history == other.history
            && self.cancel_flag.load(Ordering::SeqCst) == other.cancel_flag.load(Ordering::SeqCst)
            && self.pause_flag.load(Ordering::SeqCst) == other.pause_flag.load(Ordering::SeqCst)
    }
}

impl Eq for OperationManager {}

impl Default for OperationManager {
    fn default() -> Self {
        Self {
            active_metrics: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            pause_flag: Arc::new(AtomicBool::new(false)),
            active_conflict: None,
            active_error: None,
            bulk_conflict_resolution: None,
            history: OperationHistory::new(),
        }
    }
}

impl OperationManager {
    /// Creates a new operation manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a new operation tracking session.
    pub fn start_operation(&mut self, total_items: usize, total_bytes: u64) {
        self.cancel_flag.store(false, Ordering::SeqCst);
        self.pause_flag.store(false, Ordering::SeqCst);
        self.active_conflict = None;
        self.active_error = None;
        self.bulk_conflict_resolution = None;
        self.active_metrics = Some(OperationMetrics::new(total_items, total_bytes));
    }

    /// Requests cooperative cancellation of the running operation.
    pub fn request_cancel(&mut self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        if let Some(metrics) = self.active_metrics.as_mut() {
            metrics.is_cancelled = true;
        }
    }

    /// Toggles pause/resume state.
    pub fn toggle_pause(&mut self) {
        let current = self.pause_flag.load(Ordering::SeqCst);
        let new_state = !current;
        self.pause_flag.store(new_state, Ordering::SeqCst);
        if let Some(metrics) = self.active_metrics.as_mut() {
            metrics.is_paused = new_state;
        }
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }

    /// Whether the operation is currently paused.
    pub fn is_paused(&self) -> bool {
        self.pause_flag.load(Ordering::SeqCst)
    }

    /// Copies a single regular file with chunked transfer, progress updates, and cancellation checks.
    pub fn copy_file_chunked(&mut self, source: &Path, destination: &Path) -> Result<u64, String> {
        let mut reader = File::open(source).map_err(|e| format!("Cannot open source: {e}"))?;
        let mut writer =
            File::create(destination).map_err(|e| format!("Cannot create destination: {e}"))?;

        let mut buffer = vec![0u8; OPERATION_CHUNK_SIZE];
        let mut total_copied = 0u64;

        loop {
            if self.is_cancelled() {
                let _ = fs::remove_file(destination);
                return Err("Operation cancelled by user".to_string());
            }

            while self.is_paused() {
                std::thread::sleep(std::time::Duration::from_millis(50));
                if self.is_cancelled() {
                    let _ = fs::remove_file(destination);
                    return Err("Operation cancelled by user".to_string());
                }
            }

            let read_bytes = reader
                .read(&mut buffer)
                .map_err(|e| format!("Read error: {e}"))?;
            if read_bytes == 0 {
                break;
            }

            writer
                .write_all(&buffer[..read_bytes])
                .map_err(|e| format!("Write error: {e}"))?;

            total_copied = total_copied.saturating_add(read_bytes as u64);
            if let Some(metrics) = self.active_metrics.as_mut() {
                metrics.update_chunk(read_bytes as u64);
            }
        }

        // Copy permissions
        if let Ok(meta) = fs::metadata(source) {
            let _ = fs::set_permissions(destination, meta.permissions());
        }

        if let Some(metrics) = self.active_metrics.as_mut() {
            metrics.finish_item();
        }

        Ok(total_copied)
    }

    /// Completes the active operation, recording it in history.
    pub fn finish_operation(
        &mut self,
        kind: OperationKind,
        summary: String,
        success: bool,
        error: Option<String>,
    ) {
        if success {
            self.history.record_success(kind, summary);
        } else {
            self.history.record_failure(
                kind,
                summary,
                error.unwrap_or_else(|| "Operation aborted".to_string()),
            );
        }
        self.active_metrics = None;
        self.active_conflict = None;
        self.active_error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;

    #[test]
    fn test_chunked_file_copy() {
        let temp = TempDir::new("chunked-copy");
        let src = temp.path().join("source.dat");
        let dst = temp.path().join("dest.dat");

        let content = vec![0x42u8; 128 * 1024]; // 128 KiB
        fs::write(&src, &content).unwrap();

        let mut manager = OperationManager::new();
        manager.start_operation(1, 128 * 1024);

        let copied = manager
            .copy_file_chunked(&src, &dst)
            .expect("copy should succeed");
        assert_eq!(copied, 128 * 1024);
        assert_eq!(fs::read(&dst).unwrap(), content);
    }

    #[test]
    fn test_cancellation_cooperative() {
        let temp = TempDir::new("cancel-copy");
        let src = temp.path().join("source.dat");
        let dst = temp.path().join("dest.dat");

        fs::write(&src, vec![0x42u8; 64 * 1024]).unwrap();

        let mut manager = OperationManager::new();
        manager.start_operation(1, 64 * 1024);
        manager.request_cancel();

        let result = manager.copy_file_chunked(&src, &dst);
        assert!(result.is_err());
        assert!(
            !dst.exists(),
            "partially copied file should be cleaned up on cancellation"
        );
    }
}
