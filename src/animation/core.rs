//! Core animation data structures, lifecycles, and transitions.
//!
//! Provides the fundamental abstractions for individual animations, state tracking,
//! and value transitions.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::animation::easing::{Easing, sanitize_progress};

static NEXT_ANIMATION_ID: AtomicU64 = AtomicU64::new(1);

/// Unique numeric identifier for an animation instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnimationId(pub u64);

impl AnimationId {
    /// Generates a new globally unique `AnimationId`.
    pub fn next() -> Self {
        Self(NEXT_ANIMATION_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// Categorical tag identifying the semantic role of an animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationTag {
    /// Initial application startup and brand sequence (Phase 2.2).
    VisionBoot,
    /// Subtle visual pulse feedback for state changes.
    VisionPulse,
    /// Modal dialog entrance/exit transitions.
    Dialog,
    /// Directory and pane navigation transitions.
    Navigation,
    /// List selection and cursor transitions.
    Selection,
    /// Command Center palette transitions.
    CommandCenter,
    /// Quick Switcher jump transitions.
    QuickSwitcher,
    /// Context menu popover transitions.
    ContextMenu,
    /// File operation progress indication.
    Operation,
    /// Custom named interaction tag.
    Custom(&'static str),
}

/// Lifecycle state of an animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationState {
    /// Animation has been created but not yet updated.
    #[default]
    Created,
    /// Animation is actively running.
    Running,
    /// Animation completed its full duration.
    Completed,
    /// Animation was interrupted or cancelled before normal completion.
    Cancelled,
}

impl AnimationState {
    /// Returns `true` if the animation is currently active.
    #[inline]
    pub fn is_active(self) -> bool {
        matches!(self, Self::Created | Self::Running)
    }

    /// Returns `true` if the animation has finished (completed or cancelled).
    #[inline]
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

/// Normalized frame progress calculations for an animation instance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimationProgress {
    /// Linear normalized progress in `[0.0, 1.0]`.
    pub linear: f32,
    /// Eased progress in `[0.0, 1.0]` following the animation's easing curve.
    pub eased: f32,
    /// Elapsed duration since start.
    pub elapsed: Duration,
    /// Total configured duration.
    pub duration: Duration,
    /// Current lifecycle state.
    pub state: AnimationState,
}

impl AnimationProgress {
    /// Creates a complete progress representation for instant/completed transitions.
    pub fn complete() -> Self {
        Self {
            linear: 1.0,
            eased: 1.0,
            elapsed: Duration::ZERO,
            duration: Duration::ZERO,
            state: AnimationState::Completed,
        }
    }

    /// Creates an initial progress representation.
    pub fn initial() -> Self {
        Self {
            linear: 0.0,
            eased: 0.0,
            elapsed: Duration::ZERO,
            duration: Duration::ZERO,
            state: AnimationState::Created,
        }
    }

    /// Returns `true` if the progress has reached completion (1.0).
    #[inline]
    pub fn is_complete(&self) -> bool {
        self.linear >= 1.0 || self.state == AnimationState::Completed
    }
}

/// An individual animation track managing timing, easing, and lifecycle.
#[derive(Debug, Clone, Eq)]
pub struct Animation {
    id: AnimationId,
    tag: AnimationTag,
    start_time: Instant,
    duration: Duration,
    easing: Easing,
    state: AnimationState,
}

impl PartialEq for Animation {
    fn eq(&self, other: &Self) -> bool {
        self.tag == other.tag
            && self.duration == other.duration
            && self.easing == other.easing
            && self.state == other.state
    }
}

impl Animation {
    /// Creates a new animation starting at `start_time`.
    pub fn new(tag: AnimationTag, start_time: Instant, duration: Duration, easing: Easing) -> Self {
        Self {
            id: AnimationId::next(),
            tag,
            start_time,
            duration,
            easing,
            state: if duration.is_zero() {
                AnimationState::Completed
            } else {
                AnimationState::Created
            },
        }
    }

    /// Returns the unique ID of this animation.
    #[inline]
    pub fn id(&self) -> AnimationId {
        self.id
    }

    /// Returns the semantic tag of this animation.
    #[inline]
    pub fn tag(&self) -> AnimationTag {
        self.tag
    }

    /// Returns the configured duration.
    #[inline]
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns the easing curve.
    #[inline]
    pub fn easing(&self) -> Easing {
        self.easing
    }

    /// Returns the current lifecycle state.
    #[inline]
    pub fn state(&self) -> AnimationState {
        self.state
    }

    /// Evaluates current animation progress at timestamp `now`.
    pub fn progress(&self, now: Instant) -> AnimationProgress {
        if self.state == AnimationState::Cancelled {
            return AnimationProgress {
                linear: 1.0,
                eased: 1.0,
                elapsed: self.duration,
                duration: self.duration,
                state: AnimationState::Cancelled,
            };
        }

        if self.duration.is_zero() || self.state == AnimationState::Completed {
            return AnimationProgress {
                linear: 1.0,
                eased: 1.0,
                elapsed: self.duration,
                duration: self.duration,
                state: AnimationState::Completed,
            };
        }

        let elapsed = now.saturating_duration_since(self.start_time);
        if elapsed >= self.duration {
            AnimationProgress {
                linear: 1.0,
                eased: 1.0,
                elapsed: self.duration,
                duration: self.duration,
                state: AnimationState::Completed,
            }
        } else {
            let linear = (elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0);
            let linear = sanitize_progress(linear);
            let eased = self.easing.apply(linear);

            AnimationProgress {
                linear,
                eased,
                elapsed,
                duration: self.duration,
                state: AnimationState::Running,
            }
        }
    }

    /// Advances the internal animation lifecycle at timestamp `now`.
    pub fn update(&mut self, now: Instant) -> AnimationState {
        if self.state.is_finished() {
            return self.state;
        }

        let progress = self.progress(now);
        if progress.is_complete() {
            self.state = AnimationState::Completed;
        } else {
            self.state = AnimationState::Running;
        }
        self.state
    }

    /// Cancels the animation immediately.
    pub fn cancel(&mut self) {
        if !self.state.is_finished() {
            self.state = AnimationState::Cancelled;
        }
    }
}

/// Helper for interpolating values between `start` and `end`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition<T> {
    pub start: T,
    pub end: T,
}

impl<T> Transition<T> {
    /// Creates a new value transition.
    pub fn new(start: T, end: T) -> Self {
        Self { start, end }
    }
}

impl Transition<f32> {
    /// Interpolates float value by normalized progress.
    #[inline]
    pub fn evaluate(&self, progress: f32) -> f32 {
        let t = sanitize_progress(progress);
        self.start + (self.end - self.start) * t
    }
}

impl Transition<u16> {
    /// Interpolates u16 coordinate or dimension by normalized progress.
    #[inline]
    pub fn evaluate(&self, progress: f32) -> u16 {
        let t = sanitize_progress(progress);
        let start = self.start as f32;
        let end = self.end as f32;
        (start + (end - start) * t).round() as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_animation_lifecycle() {
        let start = Instant::now();
        let mut anim = Animation::new(
            AnimationTag::Dialog,
            start,
            Duration::from_millis(200),
            Easing::Linear,
        );

        assert_eq!(anim.state(), AnimationState::Created);

        let p0 = anim.progress(start);
        assert_eq!(p0.linear, 0.0);
        assert_eq!(p0.eased, 0.0);

        let mid = start + Duration::from_millis(100);
        let p_mid = anim.progress(mid);
        assert!((p_mid.linear - 0.5).abs() < 0.01);
        assert_eq!(anim.update(mid), AnimationState::Running);

        let end = start + Duration::from_millis(250);
        let p_end = anim.progress(end);
        assert_eq!(p_end.linear, 1.0);
        assert_eq!(anim.update(end), AnimationState::Completed);
    }

    #[test]
    fn test_animation_cancellation() {
        let start = Instant::now();
        let mut anim = Animation::new(
            AnimationTag::Dialog,
            start,
            Duration::from_millis(200),
            Easing::Linear,
        );

        anim.cancel();
        assert_eq!(anim.state(), AnimationState::Cancelled);
        assert_eq!(anim.progress(start).state, AnimationState::Cancelled);
    }

    #[test]
    fn test_transition_interpolation() {
        let trans_f32 = Transition::new(10.0, 30.0);
        assert_eq!(trans_f32.evaluate(0.0), 10.0);
        assert_eq!(trans_f32.evaluate(0.5), 20.0);
        assert_eq!(trans_f32.evaluate(1.0), 30.0);

        let trans_u16 = Transition::new(10, 50);
        assert_eq!(trans_u16.evaluate(0.0), 10);
        assert_eq!(trans_u16.evaluate(0.5), 30);
        assert_eq!(trans_u16.evaluate(1.0), 50);
    }
}
