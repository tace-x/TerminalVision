//! Smart filesystem operations and Operation Center.
//!
//! Provides chunked asynchronous operations (Copy, Move, Delete, Rename, Bulk),
//! accurate throughput metrics (percentage, speed, ETA), safe pause/resume, cooperative
//! cancellation, conflict resolution dialogues, and lightweight operation history.

pub mod conflict;
pub mod history;
pub mod manager;
pub mod progress;

pub use conflict::{ConflictPrompt, ConflictResolution, generate_unique_rename};
pub use history::{MAX_OPERATION_HISTORY, OperationHistory, OperationRecord};
pub use manager::{OPERATION_CHUNK_SIZE, OperationErrorPrompt, OperationManager};
pub use progress::{OperationMetrics, format_human_size};
