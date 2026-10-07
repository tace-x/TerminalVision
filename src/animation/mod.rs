//! TerminalVision Animation Engine & Motion Infrastructure (Phase 2.1).
//!
//! A lightweight, frame-based, deterministic, non-blocking animation system
//! designed for terminal applications with full accessibility and reduced motion support.
//!
//! # Core Design Principles
//! 1. **Terminal-Native & Non-Blocking**: No background threads or `thread::sleep`. Animations advance through event-loop frame updates.
//! 2. **Deterministic Clocks**: Supports virtual clocks (`ManualClock`) for instant, reproducible unit testing.
//! 3. **Accessibility First**: Granular motion preferences (`Full`, `Reduced`, `Off`). Disabled motion immediately presents the final state without animation frames.
//! 4. **Resize Safe**: Animation geometry is evaluated dynamically relative to the current terminal frame bounds, avoiding stale layout coordinates.
//! 5. **PTY Isolation**: Terminal emulation and user shell input remain 100% isolated and unaffected by animation execution.

pub mod boot;
pub mod clock;
pub mod core;
pub mod easing;
pub mod engine;
pub mod geometry;
pub mod micro;
pub mod preferences;

pub use boot::{
    BootPhase, BootState, CINEMATIC_BOOT_DURATION, MINIMAL_BOOT_DURATION, REDUCED_BOOT_DURATION,
    boot_duration_and_easing,
};
pub use clock::{AnimationClock, ManualClock, RealClock};
pub use core::{
    Animation, AnimationId, AnimationProgress, AnimationState, AnimationTag, Transition,
};
pub use easing::{Easing, sanitize_progress};
pub use engine::AnimationEngine;
pub use geometry::{
    clamp_rect_to_bounds, expand_rect_from_center, interpolate_rect, slide_rect_x, slide_rect_y,
};
pub use micro::{
    COMMAND_CENTER_DURATION, CONTEXT_MENU_DURATION, DIALOG_DURATION, FOCUS_DURATION,
    NAVIGATION_DURATION, OPERATION_DURATION, PREVIEW_DURATION, QUICK_SWITCHER_DURATION,
    REDUCED_MOTION_DURATION, SELECTION_DURATION, VISION_PULSE_DURATION, micro_duration_and_easing,
    trigger_micro_animation,
};
pub use preferences::{MotionMode, MotionPreferences, StartupMotionMode};
