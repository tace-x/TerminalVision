//! Integration tests for TerminalVision Phase 2.1: Animation Engine & Motion Infrastructure.

use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use terminalvision::animation::clock::ManualClock;
use terminalvision::animation::core::{AnimationState, AnimationTag, Transition};
use terminalvision::animation::easing::{Easing, sanitize_progress};
use terminalvision::animation::engine::AnimationEngine;
use terminalvision::animation::geometry::{
    clamp_rect_to_bounds, expand_rect_from_center, interpolate_rect, slide_rect_x, slide_rect_y,
};
use terminalvision::animation::preferences::{MotionMode, MotionPreferences, StartupMotionMode};
use terminalvision::app::state::App;
use terminalvision::config::Settings;

#[test]
fn test_01_linear_progress_advancement() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let mut engine = AnimationEngine::new(clock.clone(), MotionPreferences::default());

    let id = engine.start(
        AnimationTag::Navigation,
        Duration::from_millis(100),
        Easing::Linear,
    );

    let p0 = engine.progress(id).unwrap();
    assert_eq!(p0.linear, 0.0);
    assert_eq!(p0.eased, 0.0);
    assert_eq!(p0.state, AnimationState::Running);

    clock.advance(Duration::from_millis(50));
    let p50 = engine.progress(id).unwrap();
    assert!((p50.linear - 0.5).abs() < 0.001);
    assert!((p50.eased - 0.5).abs() < 0.001);

    clock.advance(Duration::from_millis(50));
    let p100 = engine.progress(id).unwrap();
    assert_eq!(p100.linear, 1.0);
    assert_eq!(p100.eased, 1.0);
    assert_eq!(p100.state, AnimationState::Completed);
}

#[test]
fn test_02_progress_clamping_and_nan_infinity_safety() {
    assert_eq!(sanitize_progress(-10.0), 0.0);
    assert_eq!(sanitize_progress(-0.0001), 0.0);
    assert_eq!(sanitize_progress(1.0001), 1.0);
    assert_eq!(sanitize_progress(500.0), 1.0);
    assert_eq!(sanitize_progress(f32::NAN), 0.0);
    assert_eq!(sanitize_progress(f32::INFINITY), 1.0);
    assert_eq!(sanitize_progress(f32::NEG_INFINITY), 0.0);

    for easing in [
        Easing::Linear,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
        Easing::CubicEaseInOut,
        Easing::SmoothStep,
    ] {
        assert_eq!(easing.apply(-5.0), 0.0);
        assert_eq!(easing.apply(10.0), 1.0);
        assert_eq!(easing.apply(f32::NAN), 0.0);
        assert_eq!(easing.apply(f32::INFINITY), 1.0);
    }
}

#[test]
fn test_03_duration_handling() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let mut engine = AnimationEngine::new(clock.clone(), MotionPreferences::default());

    let durations = [
        Duration::from_millis(1),
        Duration::from_millis(50),
        Duration::from_millis(250),
        Duration::from_secs(2),
    ];

    for dur in durations {
        let id = engine.start(AnimationTag::Custom("test"), dur, Easing::Linear);
        let p = engine.progress(id).unwrap();
        assert_eq!(p.duration, dur);
        engine.cancel(id);
    }
}

#[test]
fn test_04_completion_state_transition() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let mut engine = AnimationEngine::new(clock.clone(), MotionPreferences::default());

    let id = engine.start(
        AnimationTag::Dialog,
        Duration::from_millis(100),
        Easing::Linear,
    );

    assert!(engine.is_animating());
    assert_eq!(engine.progress(id).unwrap().state, AnimationState::Running);

    clock.advance(Duration::from_millis(100));
    assert_eq!(
        engine.progress(id).unwrap().state,
        AnimationState::Completed
    );

    // After tick, completed animation is pruned
    let is_still_running = engine.tick();
    assert!(!is_still_running);
    assert_eq!(engine.count(), 0);
    assert!(!engine.is_animating());
}

#[test]
fn test_05_cancellation_lifecycle() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let mut engine = AnimationEngine::new(clock.clone(), MotionPreferences::default());

    let id = engine.start(
        AnimationTag::Dialog,
        Duration::from_millis(300),
        Easing::EaseInOut,
    );

    clock.advance(Duration::from_millis(50));
    assert!(engine.is_animating());

    engine.cancel(id);

    // Cancelled progress reports state Cancelled and is_animating returns false
    let p = engine.progress(id).unwrap();
    assert_eq!(p.state, AnimationState::Cancelled);
    assert!(!engine.is_animating());

    // Pruning cleans it up
    engine.tick();
    assert_eq!(engine.count(), 0);
}

#[test]
fn test_06_all_easing_functions() {
    let easings = [
        (Easing::Linear, "Linear"),
        (Easing::EaseIn, "EaseIn"),
        (Easing::EaseOut, "EaseOut"),
        (Easing::EaseInOut, "EaseInOut"),
        (Easing::CubicEaseInOut, "CubicEaseInOut"),
        (Easing::SmoothStep, "SmoothStep"),
    ];

    for (easing, name) in easings {
        assert_eq!(easing.apply(0.0), 0.0, "{name} at 0.0");
        assert_eq!(easing.apply(1.0), 1.0, "{name} at 1.0");

        let mid = easing.apply(0.5);
        assert!((0.0..=1.0).contains(&mid), "{name} mid value");

        let mut prev = 0.0;
        for step in 1..=50 {
            let t = step as f32 / 50.0;
            let val = easing.apply(t);
            assert!(
                val >= prev - 0.0001,
                "{name} monotonic check at t={}: prev={}, val={}",
                t,
                prev,
                val
            );
            prev = val;
        }
    }
}

#[test]
fn test_07_reduced_motion_mode() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let prefs = MotionPreferences {
        mode: MotionMode::Reduced,
        ..Default::default()
    };

    let mut engine = AnimationEngine::new(clock.clone(), prefs);
    let id = engine.start(
        AnimationTag::Dialog,
        Duration::from_millis(500),
        Easing::Linear,
    );

    let p = engine.progress(id).unwrap();
    // Reduced motion caps duration to 60ms
    assert_eq!(p.duration, Duration::from_millis(60));

    clock.advance(Duration::from_millis(60));
    assert_eq!(
        engine.progress(id).unwrap().state,
        AnimationState::Completed
    );
}

#[test]
fn test_08_motion_off_mode() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let prefs = MotionPreferences {
        mode: MotionMode::Off,
        ..Default::default()
    };

    let mut engine = AnimationEngine::new(clock, prefs);
    let id = engine.start(
        AnimationTag::Dialog,
        Duration::from_millis(300),
        Easing::Linear,
    );

    // Motion off completes immediately in 0 frames with progress 1.0
    let p = engine.progress(id).unwrap();
    assert_eq!(p.state, AnimationState::Completed);
    assert_eq!(p.linear, 1.0);
    assert_eq!(p.eased, 1.0);
    assert!(!engine.is_animating());
}

#[test]
fn test_09_very_short_animation() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let mut engine = AnimationEngine::new(clock.clone(), MotionPreferences::default());

    let id = engine.start(
        AnimationTag::Selection,
        Duration::from_millis(5),
        Easing::Linear,
    );

    clock.advance(Duration::from_millis(5));
    let p = engine.progress(id).unwrap();
    assert_eq!(p.state, AnimationState::Completed);
    assert_eq!(p.linear, 1.0);
}

#[test]
fn test_10_zero_and_invalid_duration_handling() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let mut engine = AnimationEngine::new(clock, MotionPreferences::default());

    let id = engine.start(AnimationTag::Navigation, Duration::ZERO, Easing::Linear);

    let p = engine.progress(id).unwrap();
    assert_eq!(p.state, AnimationState::Completed);
    assert_eq!(p.linear, 1.0);
    assert_eq!(p.eased, 1.0);
    assert!(!engine.is_animating());
}

#[test]
fn test_11_resize_safe_geometry_interpolation() {
    let start = Rect::new(10, 5, 20, 10);
    let end = Rect::new(50, 20, 80, 40);
    let bounds = Rect::new(0, 0, 100, 50);

    let mid = interpolate_rect(start, end, 0.5, bounds);
    assert_eq!(mid.x, 30);
    assert_eq!(mid.y, 13);
    assert_eq!(mid.width, 50);
    assert_eq!(mid.height, 25);
    assert!(mid.x + mid.width <= bounds.width);
    assert!(mid.y + mid.height <= bounds.height);

    // Overflows are clamped strictly
    let small_bounds = Rect::new(0, 0, 40, 20);
    let clamped = interpolate_rect(start, end, 1.0, small_bounds);
    assert!(clamped.x + clamped.width <= small_bounds.width);
    assert!(clamped.y + clamped.height <= small_bounds.height);
}

#[test]
fn test_12_center_expansion_and_directional_slide() {
    let target = Rect::new(20, 10, 40, 20);
    let bounds = Rect::new(0, 0, 120, 40);

    // Center expand
    let half = expand_rect_from_center(target, 0.5, bounds);
    assert_eq!(half.width, 20);
    assert_eq!(half.height, 10);
    assert_eq!(half.x, 30);
    assert_eq!(half.y, 15);

    // Slide Y
    let slide_top = slide_rect_y(target, -10, 0.0, bounds);
    assert_eq!(slide_top.y, 0);
    let slide_mid = slide_rect_y(target, -10, 0.5, bounds);
    assert_eq!(slide_mid.y, 5);
    let slide_done = slide_rect_y(target, -10, 1.0, bounds);
    assert_eq!(slide_done.y, 10);

    // Slide X
    let slide_x = slide_rect_x(target, 20, 0.5, bounds);
    assert_eq!(slide_x.x, 30);
}

#[test]
fn test_13_multiple_concurrent_animations() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let mut engine = AnimationEngine::new(clock.clone(), MotionPreferences::default());

    let id1 = engine.start(
        AnimationTag::Navigation,
        Duration::from_millis(100),
        Easing::Linear,
    );
    let id2 = engine.start(
        AnimationTag::VisionPulse,
        Duration::from_millis(200),
        Easing::EaseOut,
    );

    assert_eq!(engine.count(), 2);
    assert!(engine.is_animating());
    assert!(engine.has_active_tag(AnimationTag::Navigation));
    assert!(engine.has_active_tag(AnimationTag::VisionPulse));

    // t = 100ms: id1 completes, id2 still running
    clock.advance(Duration::from_millis(100));
    assert_eq!(
        engine.progress(id1).unwrap().state,
        AnimationState::Completed
    );
    assert_eq!(engine.progress(id2).unwrap().state, AnimationState::Running);
    assert!(engine.tick());
    assert_eq!(engine.count(), 1);

    // t = 200ms: id2 completes
    clock.advance(Duration::from_millis(100));
    assert_eq!(
        engine.progress(id2).unwrap().state,
        AnimationState::Completed
    );
    assert!(!engine.tick());
    assert_eq!(engine.count(), 0);
    assert!(!engine.is_animating());
}

#[test]
fn test_14_tag_group_cancellation() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let mut engine = AnimationEngine::new(clock.clone(), MotionPreferences::default());

    let id1 = engine.start(
        AnimationTag::Dialog,
        Duration::from_millis(100),
        Easing::Linear,
    );
    let id2 = engine.start(
        AnimationTag::Navigation,
        Duration::from_millis(100),
        Easing::Linear,
    );

    assert_eq!(engine.count(), 2);

    engine.cancel_tag(AnimationTag::Dialog);
    assert_eq!(
        engine.progress(id1).unwrap().state,
        AnimationState::Cancelled
    );
    assert_eq!(engine.progress(id2).unwrap().state, AnimationState::Running);

    engine.tick();
    assert_eq!(engine.count(), 1);
}

#[test]
fn test_15_tag_progress_query() {
    let base = Instant::now();
    let clock = ManualClock::new(base);
    let mut engine = AnimationEngine::new(clock.clone(), MotionPreferences::default());

    assert!(engine.tag_progress(AnimationTag::CommandCenter).is_none());

    engine.start(
        AnimationTag::CommandCenter,
        Duration::from_millis(100),
        Easing::Linear,
    );

    clock.advance(Duration::from_millis(50));
    let p = engine.tag_progress(AnimationTag::CommandCenter).unwrap();
    assert!((p.linear - 0.5).abs() < 0.01);
}

#[test]
fn test_16_settings_serialization_and_deserialization_roundtrip() {
    let settings = Settings {
        motion_mode: MotionMode::Reduced,
        startup_motion: StartupMotionMode::Minimal,
        reduced_motion: true,
        ..Default::default()
    };

    let serialized = settings.serialize();
    assert!(serialized.contains("motion_mode = reduced"));
    assert!(serialized.contains("startup_motion = minimal"));

    let deserialized = Settings::deserialize(&serialized);
    assert_eq!(deserialized.motion_mode, MotionMode::Reduced);
    assert_eq!(deserialized.startup_motion, StartupMotionMode::Minimal);
    assert!(deserialized.reduced_motion);
}

#[test]
fn test_17_app_state_motion_integration() {
    let mut app = App::default();

    assert_eq!(app.motion_mode(), MotionMode::Full);
    assert!(!app.reduced_motion());
    assert!(!app.has_active_animations());

    // Toggle reduced motion
    app.set_reduced_motion(true);
    assert!(app.reduced_motion());
    assert_eq!(app.motion_mode(), MotionMode::Reduced);

    // Set motion preferences
    let prefs = MotionPreferences::new(MotionMode::Off, StartupMotionMode::Off);
    app.set_motion_preferences(prefs);
    assert_eq!(app.motion_mode(), MotionMode::Off);
    assert!(app.reduced_motion());
}

#[test]
fn test_18_small_terminal_dimensions_and_edge_cases() {
    let zero_bounds = Rect::new(0, 0, 0, 0);
    let target = Rect::new(10, 10, 20, 10);
    assert_eq!(clamp_rect_to_bounds(target, zero_bounds), Rect::default());

    let tiny_bounds = Rect::new(0, 0, 1, 1);
    let clamped_tiny = clamp_rect_to_bounds(target, tiny_bounds);
    assert!(clamped_tiny.width <= 1);
    assert!(clamped_tiny.height <= 1);

    // Standard small terminal 80x24
    let std_80x24 = Rect::new(0, 0, 80, 24);
    let inter = interpolate_rect(
        Rect::new(0, 0, 80, 24),
        Rect::new(10, 5, 60, 14),
        0.5,
        std_80x24,
    );
    assert!(inter.x + inter.width <= 80);
    assert!(inter.y + inter.height <= 24);
}

#[test]
fn test_19_transition_typed_interpolation() {
    let t_f32 = Transition::new(0.0, 100.0);
    assert_eq!(t_f32.evaluate(0.0), 0.0);
    assert_eq!(t_f32.evaluate(0.25), 25.0);
    assert_eq!(t_f32.evaluate(0.75), 75.0);
    assert_eq!(t_f32.evaluate(1.0), 100.0);

    let t_u16 = Transition::new(0, 80);
    assert_eq!(t_u16.evaluate(0.0), 0);
    assert_eq!(t_u16.evaluate(0.5), 40);
    assert_eq!(t_u16.evaluate(1.0), 80);
}

#[test]
fn test_20_frame_interval_fps_pacing() {
    let mut prefs = MotionPreferences {
        target_fps: 60,
        ..Default::default()
    };
    assert_eq!(
        prefs.frame_interval(),
        Duration::from_nanos(1_000_000_000 / 60)
    );

    prefs.target_fps = 30;
    assert_eq!(
        prefs.frame_interval(),
        Duration::from_nanos(1_000_000_000 / 30)
    );

    // Clamping to sane range [15, 120]
    prefs.target_fps = 5;
    assert_eq!(
        prefs.frame_interval(),
        Duration::from_nanos(1_000_000_000 / 15)
    );

    prefs.target_fps = 300;
    assert_eq!(
        prefs.frame_interval(),
        Duration::from_nanos(1_000_000_000 / 120)
    );
}
