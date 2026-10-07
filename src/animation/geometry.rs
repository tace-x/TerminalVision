//! Resize-safe geometry interpolation and bounding math for terminal UI animations.
//!
//! Ensures animation geometry remains strictly bounded to current terminal dimensions
//! without storing stale coordinates across terminal resizes or producing negative/overflowing Rects.

use ratatui::layout::Rect;

use crate::animation::core::Transition;
use crate::animation::easing::sanitize_progress;

/// Linearly interpolates between two `Rect` bounding boxes, clamping the result to `bounds`.
pub fn interpolate_rect(start: Rect, end: Rect, progress: f32, bounds: Rect) -> Rect {
    let t = sanitize_progress(progress);

    let x = Transition::new(start.x, end.x).evaluate(t);
    let y = Transition::new(start.y, end.y).evaluate(t);
    let width = Transition::new(start.width, end.width).evaluate(t);
    let height = Transition::new(start.height, end.height).evaluate(t);

    let rect = Rect::new(x, y, width, height);
    clamp_rect_to_bounds(rect, bounds)
}

/// Clamps a `Rect` so it never overflows the container `bounds` or produces invalid geometry.
pub fn clamp_rect_to_bounds(rect: Rect, bounds: Rect) -> Rect {
    if bounds.width == 0 || bounds.height == 0 {
        return Rect::default();
    }

    let max_x = bounds.x.saturating_add(bounds.width);
    let max_y = bounds.y.saturating_add(bounds.height);

    let x = rect.x.clamp(bounds.x, max_x.saturating_sub(1));
    let y = rect.y.clamp(bounds.y, max_y.saturating_sub(1));

    let available_w = max_x.saturating_sub(x);
    let available_h = max_y.saturating_sub(y);

    let width = rect.width.min(available_w);
    let height = rect.height.min(available_h);

    Rect::new(x, y, width, height)
}

/// Expands a `Rect` outwards from its center point towards target dimensions.
pub fn expand_rect_from_center(target: Rect, progress: f32, bounds: Rect) -> Rect {
    let t = sanitize_progress(progress);
    if t >= 1.0 {
        return clamp_rect_to_bounds(target, bounds);
    }

    let cur_w = ((target.width as f32) * t).round() as u16;
    let cur_h = ((target.height as f32) * t).round() as u16;

    let center_x = target.x + target.width / 2;
    let center_y = target.y + target.height / 2;

    let x = center_x.saturating_sub(cur_w / 2);
    let y = center_y.saturating_sub(cur_h / 2);

    let rect = Rect::new(x, y, cur_w.max(1), cur_h.max(1));
    clamp_rect_to_bounds(rect, bounds)
}

/// Slides a `Rect` vertically from an offset (e.g. entrance from top or bottom).
pub fn slide_rect_y(target: Rect, start_offset_y: i16, progress: f32, bounds: Rect) -> Rect {
    let t = sanitize_progress(progress);
    let offset = ((start_offset_y as f32) * (1.0 - t)).round() as i32;
    let y = (target.y as i32 + offset).max(0) as u16;

    let rect = Rect::new(target.x, y, target.width, target.height);
    clamp_rect_to_bounds(rect, bounds)
}

/// Slides a `Rect` horizontally from an offset (e.g. entrance from left or right).
pub fn slide_rect_x(target: Rect, start_offset_x: i16, progress: f32, bounds: Rect) -> Rect {
    let t = sanitize_progress(progress);
    let offset = ((start_offset_x as f32) * (1.0 - t)).round() as i32;
    let x = (target.x as i32 + offset).max(0) as u16;

    let rect = Rect::new(x, target.y, target.width, target.height);
    clamp_rect_to_bounds(rect, bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate_rect_clamping() {
        let start = Rect::new(0, 0, 10, 10);
        let end = Rect::new(100, 100, 50, 50);
        let bounds = Rect::new(0, 0, 80, 24);

        let mid = interpolate_rect(start, end, 0.5, bounds);
        assert!(mid.x + mid.width <= bounds.width);
        assert!(mid.y + mid.height <= bounds.height);

        let full = interpolate_rect(start, end, 1.0, bounds);
        assert!(full.x + full.width <= bounds.width);
        assert!(full.y + full.height <= bounds.height);
    }

    #[test]
    fn test_expand_from_center() {
        let target = Rect::new(20, 10, 40, 20);
        let bounds = Rect::new(0, 0, 100, 50);

        let start = expand_rect_from_center(target, 0.0, bounds);
        assert!(start.width <= target.width);
        assert!(start.height <= target.height);

        let end = expand_rect_from_center(target, 1.0, bounds);
        assert_eq!(end, target);
    }

    #[test]
    fn test_zero_bounds_safety() {
        let target = Rect::new(10, 10, 20, 20);
        let bounds = Rect::new(0, 0, 0, 0);

        let clamped = clamp_rect_to_bounds(target, bounds);
        assert_eq!(clamped, Rect::default());
    }
}
