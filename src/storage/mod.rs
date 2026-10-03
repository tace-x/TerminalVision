//! Storage Vision subsystem for directory storage analysis and heatmaps.

pub mod analysis;
pub mod heatmap;
pub mod scanner;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

pub use analysis::{StorageAnalysis, StorageChildItem, StorageFileItem, format_storage_size};
pub use heatmap::render_storage_bar;
pub use scanner::StorageScanner;

/// State for the active Storage Vision analysis session.
pub struct StorageVisionState {
    /// Currently analyzed root directory scope.
    pub current_scope: PathBuf,
    /// Navigation history stack for drilling down into folders.
    pub history_stack: Vec<PathBuf>,
    /// Prepared or completed storage analysis results.
    pub analysis: Option<StorageAnalysis>,
    /// Selected item index (for drill-down and largest items list).
    pub selected_index: usize,
    /// Whether background scanning is currently active.
    pub is_scanning: bool,
    /// Cancellation flag handle.
    pub cancel_token: Arc<AtomicBool>,
    /// Number of items scanned in live session.
    pub items_scanned: Arc<AtomicUsize>,
    /// Number of bytes scanned in live session.
    pub bytes_scanned: Arc<AtomicU64>,
    /// Receiver for completed background analysis.
    receiver: Option<Receiver<StorageAnalysis>>,
}

impl std::fmt::Debug for StorageVisionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageVisionState")
            .field("current_scope", &self.current_scope)
            .field("history_stack", &self.history_stack)
            .field("analysis", &self.analysis)
            .field("selected_index", &self.selected_index)
            .field("is_scanning", &self.is_scanning)
            .finish()
    }
}

impl Clone for StorageVisionState {
    fn clone(&self) -> Self {
        Self {
            current_scope: self.current_scope.clone(),
            history_stack: self.history_stack.clone(),
            analysis: self.analysis.clone(),
            selected_index: self.selected_index,
            is_scanning: self.is_scanning,
            cancel_token: Arc::new(AtomicBool::new(false)),
            items_scanned: Arc::new(AtomicUsize::new(0)),
            bytes_scanned: Arc::new(AtomicU64::new(0)),
            receiver: None,
        }
    }
}

impl Default for StorageVisionState {
    fn default() -> Self {
        Self {
            current_scope: PathBuf::from("."),
            history_stack: Vec::new(),
            analysis: None,
            selected_index: 0,
            is_scanning: false,
            cancel_token: Arc::new(AtomicBool::new(false)),
            items_scanned: Arc::new(AtomicUsize::new(0)),
            bytes_scanned: Arc::new(AtomicU64::new(0)),
            receiver: None,
        }
    }
}

impl PartialEq for StorageVisionState {
    fn eq(&self, other: &Self) -> bool {
        self.current_scope == other.current_scope
            && self.history_stack == other.history_stack
            && self.analysis == other.analysis
            && self.selected_index == other.selected_index
            && self.is_scanning == other.is_scanning
    }
}

impl Eq for StorageVisionState {}

impl StorageVisionState {
    /// Creates a new Storage Vision state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a background storage scan of `scope_path`.
    pub fn start_scan(&mut self, scope_path: PathBuf) {
        self.cancel();

        self.current_scope = scope_path.clone();
        self.selected_index = 0;
        self.is_scanning = true;

        let cancel_token = Arc::new(AtomicBool::new(false));
        let items_scanned = Arc::new(AtomicUsize::new(0));
        let bytes_scanned = Arc::new(AtomicU64::new(0));

        self.cancel_token = Arc::clone(&cancel_token);
        self.items_scanned = Arc::clone(&items_scanned);
        self.bytes_scanned = Arc::clone(&bytes_scanned);

        let scanner = StorageScanner::new(cancel_token, items_scanned, bytes_scanned);
        let (tx, rx): (Sender<StorageAnalysis>, Receiver<StorageAnalysis>) = channel();
        self.receiver = Some(rx);

        thread::spawn(move || {
            let analysis = scanner.scan_directory(&scope_path);
            let _ = tx.send(analysis);
        });
    }

    /// Polls background scanner for results.
    pub fn poll_updates(&mut self) {
        if let Some(ref rx) = self.receiver
            && let Ok(analysis) = rx.try_recv()
        {
            self.analysis = Some(analysis);
            self.is_scanning = false;
            self.receiver = None;
        }
    }

    /// Cancels any active background scan.
    pub fn cancel(&mut self) {
        self.cancel_token.store(true, Ordering::Relaxed);
        self.is_scanning = false;
        self.receiver = None;
    }

    /// Drills down into the currently selected direct child folder.
    pub fn drill_down(&mut self) -> bool {
        let child_opt = self.analysis.as_ref().and_then(|a| {
            a.direct_children
                .get(self.selected_index)
                .filter(|c| c.is_dir)
                .map(|c| c.path.clone())
        });

        if let Some(child_path) = child_opt {
            self.history_stack.push(self.current_scope.clone());
            self.start_scan(child_path);
            true
        } else {
            false
        }
    }

    /// Navigates up to the previous scope in history.
    pub fn go_back(&mut self) -> bool {
        if let Some(parent_scope) = self.history_stack.pop() {
            self.start_scan(parent_scope);
            true
        } else {
            false
        }
    }

    /// Moves selection up.
    pub fn move_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    /// Moves selection down.
    pub fn move_down(&mut self) {
        if let Some(analysis) = &self.analysis {
            let max_idx = analysis.direct_children.len().saturating_sub(1);
            self.selected_index = (self.selected_index + 1).min(max_idx);
        }
    }

    /// Live items scanned count.
    pub fn live_items_count(&self) -> usize {
        self.items_scanned.load(Ordering::Relaxed)
    }

    /// Live bytes scanned count.
    pub fn live_bytes_count(&self) -> u64 {
        self.bytes_scanned.load(Ordering::Relaxed)
    }

    /// Returns the target directory or file represented by the current selection or scope.
    pub fn selected_target(&self) -> PathBuf {
        if let Some(analysis) = &self.analysis
            && let Some(child) = analysis.direct_children.get(self.selected_index)
        {
            return child.path.clone();
        }
        self.current_scope.clone()
    }
}
