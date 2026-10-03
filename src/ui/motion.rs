//! Minimal, lightweight frame-based motion and transition engine for TerminalVision.
//!
//! Provides non-blocking progress calculations for subtle transitions (startup, dialogs)
//! without background threads, timers, or CPU overhead when idle.

use std::time::{Duration, Instant};

/// Standard transition durations (approximately 180–250 ms).
pub const STARTUP_DURATION: Duration = Duration::from_millis(250);
pub const DIALOG_ENTRANCE_DURATION: Duration = Duration::from_millis(180);

/// Tracks frame-based transition states for micro-interactions.
#[derive(Debug, Clone, Copy)]
pub struct MotionState {
    /// Timestamp when application started.
    pub start_time: Instant,
    /// Timestamp when current dialog or modal was opened.
    pub dialog_open_time: Option<Instant>,
    /// Whether reduced motion is enabled (skips all transitions instantly).
    pub reduced_motion: bool,
}

impl PartialEq for MotionState {
    fn eq(&self, other: &Self) -> bool {
        self.reduced_motion == other.reduced_motion
            && self.dialog_open_time.is_some() == other.dialog_open_time.is_some()
    }
}

impl Eq for MotionState {}

impl Default for MotionState {
    fn default() -> Self {
        Self {
            start_time: Instant::now(),
            dialog_open_time: None,
            reduced_motion: false,
        }
    }
}

impl MotionState {
    /// Creates a new motion tracker.
    pub fn new(reduced_motion: bool) -> Self {
        Self {
            start_time: Instant::now(),
            dialog_open_time: None,
            reduced_motion,
        }
    }

    /// Records that a dialog has just opened.
    pub fn notify_dialog_opened(&mut self) {
        if !self.reduced_motion {
            self.dialog_open_time = Some(Instant::now());
        }
    }

    /// Clears the active dialog transition.
    pub fn clear_dialog_transition(&mut self) {
        self.dialog_open_time = None;
    }

    /// Returns the progress of startup transition (0.0 to 1.0).
    pub fn startup_progress(&self, now: Instant) -> f32 {
        if self.reduced_motion {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.start_time);
        if elapsed >= STARTUP_DURATION {
            1.0
        } else {
            (elapsed.as_secs_f32() / STARTUP_DURATION.as_secs_f32()).clamp(0.0, 1.0)
        }
    }

    /// Returns the progress of the modal dialog entrance (0.0 to 1.0).
    pub fn dialog_progress(&self, now: Instant) -> f32 {
        if self.reduced_motion {
            return 1.0;
        }
        match self.dialog_open_time {
            Some(t) => {
                let elapsed = now.saturating_duration_since(t);
                if elapsed >= DIALOG_ENTRANCE_DURATION {
                    1.0
                } else {
                    (elapsed.as_secs_f32() / DIALOG_ENTRANCE_DURATION.as_secs_f32()).clamp(0.0, 1.0)
                }
            }
            None => 1.0,
        }
    }

    /// Returns `true` if any transition is currently active and advancing.
    pub fn is_active(&self, now: Instant) -> bool {
        if self.reduced_motion {
            return false;
        }
        if now.saturating_duration_since(self.start_time) < STARTUP_DURATION {
            return true;
        }
        if let Some(t) = self.dialog_open_time
            && now.saturating_duration_since(t) < DIALOG_ENTRANCE_DURATION
        {
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motion_state_instant_when_reduced_motion() {
        let mut motion = MotionState::new(true);
        let now = Instant::now();
        assert_eq!(motion.startup_progress(now), 1.0);

        motion.notify_dialog_opened();
        assert_eq!(motion.dialog_progress(now), 1.0);
        assert!(!motion.is_active(now));
    }

    #[test]
    fn test_motion_state_progress_advances() {
        let start = Instant::now();
        let mut motion = MotionState::new(false);
        motion.start_time = start;

        assert!(motion.startup_progress(start) < 0.1);
        let future = start + Duration::from_millis(300);
        assert_eq!(motion.startup_progress(future), 1.0);
    }
}
