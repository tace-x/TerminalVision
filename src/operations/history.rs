//! Lightweight recent operation history log.
//!
//! Tracks a bounded log of recent file operations (e.g. copied files, moved folders,
//! failed deletions) with timestamps and statuses without permanent database bloat.

use std::collections::VecDeque;
use std::time::SystemTime;

use crate::app::state::OperationKind;

/// Maximum number of historical operation records retained in memory.
pub const MAX_OPERATION_HISTORY: usize = 30;

/// A single recorded filesystem operation in history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRecord {
    /// The category of operation performed.
    pub kind: OperationKind,
    /// Brief description (e.g. "Copied 14 files to ~/Downloads").
    pub summary: String,
    /// Whether the operation completed successfully without errors.
    pub success: bool,
    /// When the operation completed or failed.
    pub timestamp: SystemTime,
    /// Error message if the operation failed.
    pub error_message: Option<String>,
}

/// Bounded in-memory history of recent file operations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperationHistory {
    records: VecDeque<OperationRecord>,
}

impl OperationHistory {
    /// Creates a new empty operation history.
    pub fn new() -> Self {
        Self {
            records: VecDeque::with_capacity(MAX_OPERATION_HISTORY),
        }
    }

    /// Records a successful operation.
    pub fn record_success(&mut self, kind: OperationKind, summary: impl Into<String>) {
        self.push(OperationRecord {
            kind,
            summary: summary.into(),
            success: true,
            timestamp: SystemTime::now(),
            error_message: None,
        });
    }

    /// Records a failed operation with error details.
    pub fn record_failure(
        &mut self,
        kind: OperationKind,
        summary: impl Into<String>,
        err: impl Into<String>,
    ) {
        self.push(OperationRecord {
            kind,
            summary: summary.into(),
            success: false,
            timestamp: SystemTime::now(),
            error_message: Some(err.into()),
        });
    }

    fn push(&mut self, record: OperationRecord) {
        if self.records.len() >= MAX_OPERATION_HISTORY {
            self.records.pop_back();
        }
        self.records.push_front(record);
    }

    /// Returns all recorded operations in reverse chronological order (newest first).
    pub fn entries(&self) -> impl Iterator<Item = &OperationRecord> {
        self.records.iter()
    }

    /// Number of records in history.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether history is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Clears the history log.
    pub fn clear(&mut self) {
        self.records.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_history_bounded() {
        let mut history = OperationHistory::new();
        for i in 0..50 {
            history.record_success(OperationKind::Copy, format!("Copied file {i}"));
        }

        assert_eq!(history.len(), MAX_OPERATION_HISTORY);
        assert_eq!(history.entries().next().unwrap().summary, "Copied file 49");
    }
}
