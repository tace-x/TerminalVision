//! Frame-based, deterministic, non-blocking Animation Engine.
//!
//! Coordinates all active animation tracks, applies motion preferences,
//! tracks timing via the clock abstraction, and cleans up completed tracks.

use std::time::Duration;

use crate::animation::clock::{AnimationClock, RealClock};
use crate::animation::core::{Animation, AnimationId, AnimationProgress, AnimationTag};
use crate::animation::easing::Easing;
use crate::animation::preferences::{MotionMode, MotionPreferences};

/// The centralized, non-blocking Animation Engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationEngine<C: AnimationClock = RealClock> {
    animations: Vec<Animation>,
    preferences: MotionPreferences,
    clock: C,
}

impl Default for AnimationEngine<RealClock> {
    fn default() -> Self {
        Self::new(RealClock, MotionPreferences::default())
    }
}

impl AnimationEngine<RealClock> {
    /// Creates a new `AnimationEngine` with monotonic real system clock.
    pub fn new_real(preferences: MotionPreferences) -> Self {
        Self::new(RealClock, preferences)
    }
}

impl<C: AnimationClock> AnimationEngine<C> {
    /// Creates a new `AnimationEngine` with a custom clock and motion preferences.
    pub fn new(clock: C, preferences: MotionPreferences) -> Self {
        Self {
            animations: Vec::new(),
            preferences,
            clock,
        }
    }

    /// Accesses the underlying clock reference.
    #[inline]
    pub fn clock(&self) -> &C {
        &self.clock
    }

    /// Accesses the current motion preferences.
    #[inline]
    pub fn preferences(&self) -> &MotionPreferences {
        &self.preferences
    }

    /// Mutably accesses the motion preferences.
    #[inline]
    pub fn preferences_mut(&mut self) -> &mut MotionPreferences {
        &mut self.preferences
    }

    /// Sets the motion preferences.
    pub fn set_preferences(&mut self, preferences: MotionPreferences) {
        self.preferences = preferences;
    }

    /// Sets the motion mode directly.
    pub fn set_motion_mode(&mut self, mode: MotionMode) {
        self.preferences.mode = mode;
    }

    /// Starts a new animation track.
    ///
    /// The duration is adjusted according to the active `MotionMode`.
    /// If motion is disabled (`MotionMode::Off`), the animation completes immediately in 0 frames.
    pub fn start(&mut self, tag: AnimationTag, duration: Duration, easing: Easing) -> AnimationId {
        let adjusted_duration = self.preferences.mode.adjust_duration(duration);
        let now = self.clock.now();

        // Cancel any existing running animation with the same tag
        self.cancel_tag(tag);

        let animation = Animation::new(tag, now, adjusted_duration, easing);
        let id = animation.id();
        self.animations.push(animation);
        id
    }

    /// Evaluates current progress for a given `AnimationId`.
    pub fn progress(&self, id: AnimationId) -> Option<AnimationProgress> {
        let now = self.clock.now();
        self.animations
            .iter()
            .find(|a| a.id() == id)
            .map(|a| a.progress(now))
    }

    /// Evaluates current progress for the active animation with `tag`.
    pub fn tag_progress(&self, tag: AnimationTag) -> Option<AnimationProgress> {
        let now = self.clock.now();
        self.animations
            .iter()
            .rev()
            .find(|a| a.tag() == tag)
            .map(|a| a.progress(now))
    }

    /// Returns `true` if any animation is currently active and running.
    pub fn is_animating(&self) -> bool {
        if !self.preferences.is_animated() {
            return false;
        }
        let now = self.clock.now();
        self.animations
            .iter()
            .any(|a| a.progress(now).state.is_active())
    }

    /// Returns `true` if an animation with `tag` is currently active.
    pub fn has_active_tag(&self, tag: AnimationTag) -> bool {
        if !self.preferences.is_animated() {
            return false;
        }
        let now = self.clock.now();
        self.animations
            .iter()
            .any(|a| a.tag() == tag && a.progress(now).state.is_active())
    }

    /// Cancels an animation by ID.
    pub fn cancel(&mut self, id: AnimationId) {
        if let Some(anim) = self.animations.iter_mut().find(|a| a.id() == id) {
            anim.cancel();
        }
    }

    /// Cancels all active animations matching `tag`.
    pub fn cancel_tag(&mut self, tag: AnimationTag) {
        for anim in &mut self.animations {
            if anim.tag() == tag && anim.state().is_active() {
                anim.cancel();
            }
        }
    }

    /// Cancels all active animations across the entire engine.
    pub fn cancel_all(&mut self) {
        for anim in &mut self.animations {
            anim.cancel();
        }
    }

    /// Updates all active animations against the clock, advancing lifecycles and pruning finished tracks.
    ///
    /// Returns `true` if any animation updated its frame state or is currently running.
    pub fn tick(&mut self) -> bool {
        let now = self.clock.now();
        let mut any_active = false;

        for anim in &mut self.animations {
            let state = anim.update(now);
            if state.is_active() {
                any_active = true;
            }
        }

        // Retain running animations and prune finished/cancelled ones
        self.animations.retain(|a| a.state().is_active());

        any_active
    }

    /// Clears all animation tracks immediately.
    pub fn clear(&mut self) {
        self.animations.clear();
    }

    /// Returns the number of currently tracked animations.
    #[inline]
    pub fn count(&self) -> usize {
        self.animations.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::clock::ManualClock;
    use crate::animation::core::AnimationState;
    use std::time::Instant;

    #[test]
    fn test_engine_full_lifecycle() {
        let base = Instant::now();
        let clock = ManualClock::new(base);
        let mut engine = AnimationEngine::new(clock.clone(), MotionPreferences::default());

        let id = engine.start(
            AnimationTag::Dialog,
            Duration::from_millis(100),
            Easing::Linear,
        );

        assert_eq!(engine.count(), 1);
        assert!(engine.is_animating());

        // At t = 50ms
        clock.advance(Duration::from_millis(50));
        assert!(engine.tick());
        let p = engine.progress(id).unwrap();
        assert!((p.linear - 0.5).abs() < 0.05);

        // At t = 110ms (completed)
        clock.advance(Duration::from_millis(60));
        assert!(!engine.tick());
        assert_eq!(engine.count(), 0);
        assert!(!engine.is_animating());
    }

    #[test]
    fn test_engine_reduced_motion() {
        let base = Instant::now();
        let clock = ManualClock::new(base);
        let prefs = MotionPreferences {
            mode: MotionMode::Reduced,
            ..Default::default()
        };

        let mut engine = AnimationEngine::new(clock.clone(), prefs);
        let id = engine.start(
            AnimationTag::Dialog,
            Duration::from_millis(400),
            Easing::Linear,
        );

        // Under Reduced motion, 400ms duration is compressed to 60ms
        let p0 = engine.progress(id).unwrap();
        assert_eq!(p0.duration, Duration::from_millis(60));
    }

    #[test]
    fn test_engine_motion_off() {
        let base = Instant::now();
        let clock = ManualClock::new(base);
        let prefs = MotionPreferences {
            mode: MotionMode::Off,
            ..Default::default()
        };

        let mut engine = AnimationEngine::new(clock, prefs);
        let id = engine.start(
            AnimationTag::Dialog,
            Duration::from_millis(200),
            Easing::Linear,
        );

        let p = engine.progress(id).unwrap();
        assert_eq!(p.state, AnimationState::Completed);
        assert_eq!(p.linear, 1.0);
        assert!(!engine.is_animating());
    }

    #[test]
    fn test_engine_cancellation() {
        let base = Instant::now();
        let clock = ManualClock::new(base);
        let mut engine = AnimationEngine::new(clock.clone(), MotionPreferences::default());

        let id = engine.start(
            AnimationTag::Dialog,
            Duration::from_millis(200),
            Easing::Linear,
        );

        engine.cancel(id);
        assert!(!engine.is_animating());
        engine.tick();
        assert_eq!(engine.count(), 0);
    }
}
