//! Easing functions for terminal-native, smooth animations.
//!
//! Provides deterministic progress transformations ensuring inputs and outputs
//! stay strictly bounded in [0.0, 1.0] with zero NaN, Infinity, or negative values.

/// Supported easing curves for terminal micro-interactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    /// Constant rate of change (f(t) = t).
    #[default]
    Linear,
    /// Quadratic acceleration from zero velocity (f(t) = t^2).
    EaseIn,
    /// Quadratic deceleration to zero velocity (f(t) = 1 - (1 - t)^2).
    EaseOut,
    /// Quadratic acceleration followed by deceleration.
    EaseInOut,
    /// Cubic acceleration followed by deceleration for smoother inflection.
    CubicEaseInOut,
    /// Hermite smoothstep interpolation (f(t) = 3t^2 - 2t^3).
    SmoothStep,
}

impl Easing {
    /// Applies the easing function to normalized progress `t` in `[0.0, 1.0]`.
    ///
    /// The input is automatically clamped, sanitized against NaN/Infinity,
    /// and the returned value is guaranteed to be in `[0.0, 1.0]`.
    pub fn apply(self, t: f32) -> f32 {
        let clamped = sanitize_progress(t);
        let result = match self {
            Self::Linear => clamped,
            Self::EaseIn => clamped * clamped,
            Self::EaseOut => 1.0 - (1.0 - clamped) * (1.0 - clamped),
            Self::EaseInOut => {
                if clamped < 0.5 {
                    2.0 * clamped * clamped
                } else {
                    1.0 - (-2.0 * clamped + 2.0).powi(2) / 2.0
                }
            }
            Self::CubicEaseInOut => {
                if clamped < 0.5 {
                    4.0 * clamped * clamped * clamped
                } else {
                    1.0 - (-2.0 * clamped + 2.0).powi(3) / 2.0
                }
            }
            Self::SmoothStep => clamped * clamped * (3.0 - 2.0 * clamped),
        };

        sanitize_progress(result)
    }
}

/// Sanitizes and clamps progress to strictly `[0.0, 1.0]`.
///
/// Converts NaNs and negative values to `0.0`, and values $> 1.0$ or $+\infty$ to `1.0`.
#[inline]
pub fn sanitize_progress(t: f32) -> f32 {
    if t.is_nan() || t <= 0.0 {
        0.0
    } else if t >= 1.0 || t.is_infinite() {
        1.0
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_easing_boundaries() {
        let easings = [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::CubicEaseInOut,
            Easing::SmoothStep,
        ];

        for easing in easings {
            assert_eq!(easing.apply(0.0), 0.0);
            assert_eq!(easing.apply(1.0), 1.0);
            assert_eq!(easing.apply(-0.5), 0.0);
            assert_eq!(easing.apply(1.5), 1.0);
            assert_eq!(easing.apply(f32::NAN), 0.0);
            assert_eq!(easing.apply(f32::INFINITY), 1.0);
            assert_eq!(easing.apply(f32::NEG_INFINITY), 0.0);

            // Midpoint monotonicity
            let mid = easing.apply(0.5);
            assert!((0.0..=1.0).contains(&mid));
        }
    }

    #[test]
    fn test_easing_monotonicity() {
        let easings = [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::CubicEaseInOut,
            Easing::SmoothStep,
        ];

        for easing in easings {
            let mut prev = 0.0;
            for i in 1..=100 {
                let t = i as f32 / 100.0;
                let val = easing.apply(t);
                assert!(
                    val >= prev,
                    "Easing {:?} failed monotonicity at t={}: prev={}, val={}",
                    easing,
                    t,
                    prev,
                    val
                );
                prev = val;
            }
        }
    }
}
