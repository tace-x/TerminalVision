//! Deterministic clock abstraction for Animation Engine.
//!
//! Decouples real-time physical clocks from animation progress calculations,
//! enabling fast, deterministic, non-flaky unit tests.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Trait abstracting the time source for animations.
pub trait AnimationClock: Send + Sync {
    /// Returns the current timestamp.
    fn now(&self) -> Instant;
}

/// Standard production clock relying on monotonic system time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RealClock;

impl AnimationClock for RealClock {
    #[inline]
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Deterministic, controllable clock for unit tests and simulation.
///
/// Internally tracks elapsed nanoseconds relative to an arbitrary base `Instant`.
#[derive(Debug, Clone)]
pub struct ManualClock {
    base: Instant,
    nanos_offset: Arc<AtomicU64>,
}

impl PartialEq for ManualClock {
    fn eq(&self, other: &Self) -> bool {
        self.now() == other.now()
    }
}

impl Eq for ManualClock {}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

impl ManualClock {
    /// Creates a manual clock fixed at `base`.
    pub fn new(base: Instant) -> Self {
        Self {
            base,
            nanos_offset: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Advances the manual clock by `duration`.
    pub fn advance(&self, duration: Duration) {
        let nanos = duration.as_nanos() as u64;
        self.nanos_offset.fetch_add(nanos, Ordering::SeqCst);
    }

    /// Sets the total elapsed offset from base.
    pub fn set_elapsed(&self, elapsed: Duration) {
        let nanos = elapsed.as_nanos() as u64;
        self.nanos_offset.store(nanos, Ordering::SeqCst);
    }

    /// Resets elapsed offset to 0.
    pub fn reset(&self) {
        self.nanos_offset.store(0, Ordering::SeqCst);
    }
}

impl AnimationClock for ManualClock {
    fn now(&self) -> Instant {
        let offset = self.nanos_offset.load(Ordering::SeqCst);
        self.base + Duration::from_nanos(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manual_clock_advancement() {
        let base = Instant::now();
        let clock = ManualClock::new(base);

        assert_eq!(clock.now(), base);

        clock.advance(Duration::from_millis(150));
        assert_eq!(clock.now(), base + Duration::from_millis(150));

        clock.advance(Duration::from_millis(250));
        assert_eq!(clock.now(), base + Duration::from_millis(400));

        clock.reset();
        assert_eq!(clock.now(), base);
    }
}
