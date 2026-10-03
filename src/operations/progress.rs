//! Progress calculation and throughput metrics for file operations.
//!
//! Provides accurate computation of bytes completed, total bytes, transfer speed (B/s),
//! estimated time of arrival (ETA), item counts, and progress rendering.

use std::time::Instant;

/// Snapshot of metrics for an ongoing or completed filesystem operation.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationMetrics {
    /// Number of bytes processed so far.
    pub bytes_completed: u64,
    /// Total bytes to process (0 if indeterminate).
    pub total_bytes: u64,
    /// Number of individual items (files/folders) processed so far.
    pub items_completed: usize,
    /// Total number of items in this operation.
    pub total_items: usize,
    /// When the operation was started.
    pub start_time: Instant,
    /// Time of last chunk update.
    pub last_update: Instant,
    /// Instantaneous or smoothed transfer speed in bytes per second.
    pub speed_bytes_per_sec: f64,
    /// Current source item path or name.
    pub current_source: Option<String>,
    /// Current destination path or name.
    pub current_destination: Option<String>,
    /// Whether the operation is currently paused.
    pub is_paused: bool,
    /// Whether the operation was cancelled.
    pub is_cancelled: bool,
}

impl Eq for OperationMetrics {}

impl OperationMetrics {
    /// Creates a new metrics tracker with total item and byte targets.
    pub fn new(total_items: usize, total_bytes: u64) -> Self {
        let now = Instant::now();
        Self {
            bytes_completed: 0,
            total_bytes,
            items_completed: 0,
            total_items,
            start_time: now,
            last_update: now,
            speed_bytes_per_sec: 0.0,
            current_source: None,
            current_destination: None,
            is_paused: false,
            is_cancelled: false,
        }
    }

    /// Updates metrics after processing a chunk of `bytes`.
    pub fn update_chunk(&mut self, bytes: u64) {
        self.bytes_completed = self.bytes_completed.saturating_add(bytes);
        let now = Instant::now();
        let total_elapsed = now.duration_since(self.start_time).as_secs_f64();

        if total_elapsed > 0.05 {
            self.speed_bytes_per_sec = (self.bytes_completed as f64) / total_elapsed;
        }
        self.last_update = now;
    }

    /// Increments item completion counter.
    pub fn finish_item(&mut self) {
        self.items_completed = self.items_completed.saturating_add(1);
    }

    /// Sets the currently active source and destination descriptions.
    pub fn set_current_item(&mut self, source: Option<String>, destination: Option<String>) {
        self.current_source = source;
        self.current_destination = destination;
    }

    /// Computes percentage completion (0.0 to 100.0).
    pub fn percentage(&self) -> f64 {
        if self.total_bytes > 0 {
            let pct = (self.bytes_completed as f64 / self.total_bytes as f64) * 100.0;
            pct.clamp(0.0, 100.0)
        } else if self.total_items > 0 {
            let pct = (self.items_completed as f64 / self.total_items as f64) * 100.0;
            pct.clamp(0.0, 100.0)
        } else {
            0.0
        }
    }

    /// Computes estimated remaining time in seconds.
    pub fn eta_seconds(&self) -> Option<f64> {
        if self.total_bytes > 0 && self.speed_bytes_per_sec > 1024.0 {
            let remaining_bytes = self.total_bytes.saturating_sub(self.bytes_completed);
            if remaining_bytes == 0 {
                return Some(0.0);
            }
            Some(remaining_bytes as f64 / self.speed_bytes_per_sec)
        } else {
            None
        }
    }

    /// Formats transfer speed into a human-readable string (e.g. "18.4 MB/s").
    pub fn format_speed(&self) -> String {
        if self.speed_bytes_per_sec < 1024.0 {
            format!("{:.0} B/s", self.speed_bytes_per_sec)
        } else if self.speed_bytes_per_sec < 1024.0 * 1024.0 {
            format!("{:.1} KB/s", self.speed_bytes_per_sec / 1024.0)
        } else if self.speed_bytes_per_sec < 1024.0 * 1024.0 * 1024.0 {
            format!("{:.1} MB/s", self.speed_bytes_per_sec / (1024.0 * 1024.0))
        } else {
            format!(
                "{:.2} GB/s",
                self.speed_bytes_per_sec / (1024.0 * 1024.0 * 1024.0)
            )
        }
    }

    /// Formats ETA into human-readable notation (e.g. "ETA 1.1s" or "ETA 2m 14s").
    pub fn format_eta(&self) -> String {
        match self.eta_seconds() {
            Some(secs) if secs < 1.0 => "ETA <1s".to_string(),
            Some(secs) if secs < 60.0 => format!("ETA {:.1}s", secs),
            Some(secs) => {
                let mins = (secs / 60.0).floor() as u64;
                let rem_secs = (secs % 60.0).round() as u64;
                format!("ETA {mins}m {rem_secs}s")
            }
            None => "ETA --".to_string(),
        }
    }

    /// Formats bytes completed vs total (e.g. "42.1 MB / 62.8 MB").
    pub fn format_bytes_progress(&self) -> String {
        if self.total_bytes > 0 {
            format!(
                "{} / {}",
                format_human_size(self.bytes_completed),
                format_human_size(self.total_bytes)
            )
        } else {
            format!(
                "{} items / {} total",
                self.items_completed, self.total_items
            )
        }
    }

    /// Renders an ASCII/Unicode progress bar of given `width`.
    pub fn render_progress_bar(&self, width: usize) -> String {
        if width == 0 {
            return String::new();
        }
        let pct = self.percentage() / 100.0;
        let filled_chars = ((width as f64) * pct).round() as usize;
        let filled_chars = filled_chars.min(width);
        let empty_chars = width.saturating_sub(filled_chars);

        let mut bar = String::with_capacity(width * 4);
        for _ in 0..filled_chars {
            bar.push('█');
        }
        for _ in 0..empty_chars {
            bar.push('░');
        }
        bar
    }
}

/// Formats a byte size into a compact human-readable string (B, KB, MB, GB).
pub fn format_human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_metrics_progress_bar() {
        let mut metrics = OperationMetrics::new(1, 1000);
        metrics.update_chunk(500);

        assert!((metrics.percentage() - 50.0).abs() < 0.1);
        let bar = metrics.render_progress_bar(10);
        assert_eq!(bar, "█████░░░░░");
    }

    #[test]
    fn test_human_size_formatting() {
        assert_eq!(format_human_size(500), "500 B");
        assert_eq!(format_human_size(1500), "1.5 KB");
        assert_eq!(format_human_size(1024 * 1024 * 42), "42.0 MB");
    }
}
