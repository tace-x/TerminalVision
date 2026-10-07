//! Signature Motion and Micro-Interactions timing, easing, and triggers.
//!
//! Governs the lifecycle and progression of subtle, non-blocking UI transitions
//! including navigation, selection, focus, modals, command palette, quick switcher,
//! context menus, operation feedback, and the reusable Vision Pulse.

use std::time::Duration;

use crate::animation::core::{AnimationId, AnimationTag};
use crate::animation::easing::Easing;
use crate::animation::engine::AnimationEngine;
use crate::animation::preferences::{MotionMode, MotionPreferences};

/// Standard duration for directory and path navigation transitions (~140ms).
pub const NAVIGATION_DURATION: Duration = Duration::from_millis(140);

/// Standard duration for file selection and cursor movements (~100ms).
pub const SELECTION_DURATION: Duration = Duration::from_millis(100);

/// Standard duration for pane and widget focus transitions (~120ms).
pub const FOCUS_DURATION: Duration = Duration::from_millis(120);

/// Standard duration for Command Center palette entrance (~150ms).
pub const COMMAND_CENTER_DURATION: Duration = Duration::from_millis(150);

/// Standard duration for Quick Switcher modal entrance (~150ms).
pub const QUICK_SWITCHER_DURATION: Duration = Duration::from_millis(150);

/// Standard duration for modal dialogs and confirmation prompts (~150ms).
pub const DIALOG_DURATION: Duration = Duration::from_millis(150);

/// Standard duration for context menu popovers (~120ms).
pub const CONTEXT_MENU_DURATION: Duration = Duration::from_millis(120);

/// Standard duration for preview content updates (~120ms).
pub const PREVIEW_DURATION: Duration = Duration::from_millis(120);

/// Standard duration for operation progress and state feedback (~160ms).
pub const OPERATION_DURATION: Duration = Duration::from_millis(160);

/// Standard duration for the signature Vision Pulse effect (~220ms).
pub const VISION_PULSE_DURATION: Duration = Duration::from_millis(220);

/// Reduced motion capped duration (~70ms).
pub const REDUCED_MOTION_DURATION: Duration = Duration::from_millis(70);

/// Computes the configured duration and easing curve for a given [`AnimationTag`].
pub fn micro_duration_and_easing(
    tag: AnimationTag,
    prefs: &MotionPreferences,
) -> Option<(Duration, Easing)> {
    if !prefs.is_animated() {
        return None;
    }

    let (base_duration, easing) = match tag {
        AnimationTag::Navigation => (NAVIGATION_DURATION, Easing::EaseOut),
        AnimationTag::Selection => (SELECTION_DURATION, Easing::EaseOut),
        AnimationTag::CommandCenter => (COMMAND_CENTER_DURATION, Easing::EaseOut),
        AnimationTag::QuickSwitcher => (QUICK_SWITCHER_DURATION, Easing::EaseOut),
        AnimationTag::Dialog => (DIALOG_DURATION, Easing::EaseOut),
        AnimationTag::ContextMenu => (CONTEXT_MENU_DURATION, Easing::EaseOut),
        AnimationTag::Operation => (OPERATION_DURATION, Easing::EaseOut),
        AnimationTag::VisionPulse => (VISION_PULSE_DURATION, Easing::CubicEaseInOut),
        AnimationTag::VisionBoot => return None, // Handled by boot module
        AnimationTag::Custom("Preview") => (PREVIEW_DURATION, Easing::EaseOut),
        AnimationTag::Custom("Focus") => (FOCUS_DURATION, Easing::EaseOut),
        AnimationTag::Custom(_) => (DIALOG_DURATION, Easing::EaseOut),
    };

    let easing = if prefs.mode == MotionMode::Reduced {
        Easing::Linear
    } else {
        easing
    };

    Some((base_duration, easing))
}

/// Triggers a micro-interaction animation in the [`AnimationEngine`].
///
/// Automatically supersedes/replaces any previous animation for the same tag,
/// guaranteeing rapid keyboard/mouse input never queues or creates latency.
pub fn trigger_micro_animation<C: crate::animation::clock::AnimationClock>(
    engine: &mut AnimationEngine<C>,
    tag: AnimationTag,
) -> Option<AnimationId> {
    let (duration, easing) = micro_duration_and_easing(tag, engine.preferences())?;
    Some(engine.start(tag, duration, easing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::clock::ManualClock;
    use std::time::Instant;

    #[test]
    fn test_micro_durations_full_motion() {
        let prefs = MotionPreferences {
            mode: MotionMode::Full,
            ..Default::default()
        };

        let (dur, easing) = micro_duration_and_easing(AnimationTag::Navigation, &prefs).unwrap();
        assert_eq!(dur, NAVIGATION_DURATION);
        assert_eq!(easing, Easing::EaseOut);

        let (dur, easing) = micro_duration_and_easing(AnimationTag::VisionPulse, &prefs).unwrap();
        assert_eq!(dur, VISION_PULSE_DURATION);
        assert_eq!(easing, Easing::CubicEaseInOut);
    }

    #[test]
    fn test_micro_durations_reduced_motion() {
        let prefs = MotionPreferences {
            mode: MotionMode::Reduced,
            ..Default::default()
        };

        let (dur, easing) = micro_duration_and_easing(AnimationTag::Navigation, &prefs).unwrap();
        assert_eq!(dur, NAVIGATION_DURATION);
        assert_eq!(easing, Easing::Linear);
    }

    #[test]
    fn test_micro_durations_off_motion() {
        let prefs = MotionPreferences {
            mode: MotionMode::Off,
            ..Default::default()
        };

        assert!(micro_duration_and_easing(AnimationTag::Navigation, &prefs).is_none());
        assert!(micro_duration_and_easing(AnimationTag::Selection, &prefs).is_none());
    }

    #[test]
    fn test_trigger_micro_animation_replacement() {
        let base = Instant::now();
        let clock = ManualClock::new(base);
        let prefs = MotionPreferences::default();
        let mut engine = AnimationEngine::new(clock.clone(), prefs);

        let id1 = trigger_micro_animation(&mut engine, AnimationTag::Selection).unwrap();
        assert!(engine.is_animating());

        // Triggering again supersedes previous
        let id2 = trigger_micro_animation(&mut engine, AnimationTag::Selection).unwrap();
        assert_ne!(id1, id2);

        // Advance clock past duration
        clock.advance(Duration::from_millis(150));
        assert!(!engine.is_animating());
    }
}
