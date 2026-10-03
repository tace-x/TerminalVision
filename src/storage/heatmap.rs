//! Heatmap and visual bar chart generation for storage analysis.
//!
//! Renders clean, proportional Unicode horizontal bar charts for directory usage.

/// Renders a horizontal Unicode progress/heatmap bar for a given `percentage` (0.0 to 100.0)
/// scaled to `bar_width` character cells.
pub fn render_storage_bar(percentage: f32, bar_width: usize) -> String {
    if bar_width == 0 {
        return String::new();
    }

    let clamped = percentage.clamp(0.0, 100.0);
    let fill_cells = ((clamped / 100.0) * (bar_width as f32)).round() as usize;
    let fill_cells = fill_cells.min(bar_width);
    let empty_cells = bar_width.saturating_sub(fill_cells);

    let mut bar = String::with_capacity(bar_width * 4);
    for _ in 0..fill_cells {
        bar.push('█');
    }
    for _ in 0..empty_cells {
        bar.push('░');
    }

    bar
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_storage_bar_lengths() {
        let bar_100 = render_storage_bar(100.0, 10);
        assert_eq!(bar_100.chars().count(), 10);
        assert_eq!(bar_100, "██████████");

        let bar_50 = render_storage_bar(50.0, 10);
        assert_eq!(bar_50.chars().count(), 10);
        assert_eq!(bar_50, "█████░░░░░");

        let bar_0 = render_storage_bar(0.0, 10);
        assert_eq!(bar_0.chars().count(), 10);
        assert_eq!(bar_0, "░░░░░░░░░░");
    }
}
