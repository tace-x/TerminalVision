//! User motion preferences and accessibility controls.
//!
//! Provides granular user options for motion fidelity, respecting accessibility
//! needs without ever requiring animations to understand terminal state.

use std::str::FromStr;
use std::time::Duration;

/// Motion fidelity modes controlling animation playback and duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MotionMode {
    /// Full fidelity: standard animated transitions and micro-interactions.
    #[default]
    Full,
    /// Reduced motion: short, minimal transitions (durations compressed).
    Reduced,
    /// Motion disabled: immediate 0ms state changes, zero animation frames.
    Off,
}

impl MotionMode {
    /// Returns `true` if motion animations are enabled (`Full` or `Reduced`).
    #[inline]
    pub fn is_enabled(self) -> bool {
        self != Self::Off
    }

    /// Returns `true` if reduced motion is active (`Reduced` or `Off`).
    #[inline]
    pub fn is_reduced_or_off(self) -> bool {
        self != Self::Full
    }

    /// Adjusts an animation duration based on the active motion preference.
    pub fn adjust_duration(self, original: Duration) -> Duration {
        match self {
            Self::Full => original,
            Self::Reduced => {
                // Shorten transition to at most 60ms or 25% of original duration
                let quarter = original / 4;
                quarter.min(Duration::from_millis(60))
            }
            Self::Off => Duration::ZERO,
        }
    }

    /// Returns the string representation for configuration serialization.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Reduced => "reduced",
            Self::Off => "off",
        }
    }
}

impl FromStr for MotionMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "full" | "normal" | "on" | "true" => Ok(Self::Full),
            "reduced" | "minimal" => Ok(Self::Reduced),
            "off" | "none" | "false" | "disabled" => Ok(Self::Off),
            _ => Err(()),
        }
    }
}

/// Startup motion preference preparation for Phase 2.2 (VISION BOOT).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StartupMotionMode {
    /// Full cinematic signature boot sequence.
    #[default]
    Cinematic,
    /// Fast minimal signature boot indication.
    Minimal,
    /// Instant startup without motion sequence.
    Off,
}

impl StartupMotionMode {
    /// Returns the string representation for configuration serialization.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cinematic => "cinematic",
            Self::Minimal => "minimal",
            Self::Off => "off",
        }
    }
}

impl FromStr for StartupMotionMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "cinematic" | "full" | "true" => Ok(Self::Cinematic),
            "minimal" | "fast" | "short" => Ok(Self::Minimal),
            "off" | "none" | "false" | "disabled" => Ok(Self::Off),
            _ => Err(()),
        }
    }
}

/// Complete motion preferences configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionPreferences {
    /// General UI transition motion fidelity.
    pub mode: MotionMode,
    /// Startup sequence motion mode.
    pub startup: StartupMotionMode,
    /// Target frame rate in FPS (e.g., 60).
    pub target_fps: u32,
}

impl Default for MotionPreferences {
    fn default() -> Self {
        Self {
            mode: MotionMode::Full,
            startup: StartupMotionMode::Cinematic,
            target_fps: 60,
        }
    }
}

impl MotionPreferences {
    /// Creates motion preferences with default 60 FPS target.
    pub fn new(mode: MotionMode, startup: StartupMotionMode) -> Self {
        Self {
            mode,
            startup,
            target_fps: 60,
        }
    }

    /// Returns the interval between frame ticks during active animations.
    pub fn frame_interval(&self) -> Duration {
        let fps = self.target_fps.clamp(15, 120);
        Duration::from_nanos(1_000_000_000 / fps as u64)
    }

    /// Whether animations should execute.
    #[inline]
    pub fn is_animated(&self) -> bool {
        self.mode.is_enabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motion_mode_duration_adjustment() {
        let original = Duration::from_millis(200);

        assert_eq!(MotionMode::Full.adjust_duration(original), original);
        assert_eq!(
            MotionMode::Reduced.adjust_duration(original),
            Duration::from_millis(50)
        );
        assert_eq!(MotionMode::Off.adjust_duration(original), Duration::ZERO);
    }

    #[test]
    fn test_motion_mode_parsing() {
        assert_eq!("full".parse::<MotionMode>(), Ok(MotionMode::Full));
        assert_eq!("reduced".parse::<MotionMode>(), Ok(MotionMode::Reduced));
        assert_eq!("off".parse::<MotionMode>(), Ok(MotionMode::Off));
        assert_eq!("true".parse::<MotionMode>(), Ok(MotionMode::Full));
        assert_eq!("false".parse::<MotionMode>(), Ok(MotionMode::Off));
    }

    #[test]
    fn test_startup_motion_parsing() {
        assert_eq!(
            "cinematic".parse::<StartupMotionMode>(),
            Ok(StartupMotionMode::Cinematic)
        );
        assert_eq!(
            "minimal".parse::<StartupMotionMode>(),
            Ok(StartupMotionMode::Minimal)
        );
        assert_eq!(
            "off".parse::<StartupMotionMode>(),
            Ok(StartupMotionMode::Off)
        );
    }
}
