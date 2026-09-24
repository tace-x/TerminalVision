//! Responsive breakpoints and layout modes.
//!
//! How much of the terminal is available decides how the interface adapts
//! its content, regions, headers, and footers. The decision considers both
//! terminal width and height so that the interface feels intentionally
//! designed at every terminal size.

use ratatui::layout::Rect;

/// Breakpoint column widths for responsive modes.
pub const BREAKPOINT_COMPACT: u16 = 60;
pub const BREAKPOINT_DUAL_PANE: u16 = 90;
pub const BREAKPOINT_DUAL_COMPACT_PREVIEW: u16 = 120;
pub const BREAKPOINT_DUAL_PREVIEW: u16 = 160;
pub const BREAKPOINT_DEVELOPER_WIDE: u16 = 220;
pub const BREAKPOINT_ULTRA_WIDE: u16 = 300;

/// The narrowest terminal that can show two panes.
pub const TWO_PANE_MIN_WIDTH: u16 = BREAKPOINT_DUAL_PANE;

/// The narrowest terminal that can show a preview beside the panes.
pub const PREVIEW_MIN_WIDTH: u16 = BREAKPOINT_DUAL_COMPACT_PREVIEW;

/// Granular responsive tiers considering both terminal width and height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResponsiveTier {
    /// Emergency minimal layout on severely cramped screens (< 30 cols or < 4 rows).
    Emergency,
    /// Compact single-pane layout on small screens (30..60 cols or < 6 rows).
    Compact,
    /// Standard single-pane layout (60..90 cols).
    SinglePane,
    /// Dual-pane layout without preview (90..120 cols or short terminals).
    DualPane,
    /// Dual-pane with a compact preview pane (120..160 cols).
    DualCompactPreview,
    /// Dual-pane with full preview pane (160..220 cols).
    DualPreview,
    /// Developer wide layout (220..300 cols).
    DeveloperWide,
    /// Ultra wide layout with expansive panes (300+ cols).
    UltraWide,
}

impl ResponsiveTier {
    /// Determines the appropriate responsive tier for a given terminal area.
    ///
    /// Considers both width and height: if height is severely constrained,
    /// the layout gracefully degrades to avoid squeezing vertical content.
    pub const fn calculate(area: Rect) -> Self {
        let width = area.width;
        let height = area.height;

        if height < 4 || width < 30 {
            Self::Emergency
        } else if width < BREAKPOINT_COMPACT {
            Self::Compact
        } else if width < BREAKPOINT_DUAL_PANE {
            Self::SinglePane
        } else if height < 6 {
            // Short terminal height cannot support preview side-by-side cleanly
            if width < BREAKPOINT_DEVELOPER_WIDE {
                Self::DualPane
            } else {
                Self::DeveloperWide
            }
        } else if width < BREAKPOINT_DUAL_COMPACT_PREVIEW {
            Self::DualPane
        } else if width < BREAKPOINT_DUAL_PREVIEW {
            Self::DualCompactPreview
        } else if width < BREAKPOINT_DEVELOPER_WIDE {
            Self::DualPreview
        } else if width < BREAKPOINT_ULTRA_WIDE {
            Self::DeveloperWide
        } else {
            Self::UltraWide
        }
    }

    /// Maps this responsive tier to the corresponding high-level [`LayoutMode`].
    pub const fn layout_mode(self) -> LayoutMode {
        match self {
            Self::Emergency | Self::Compact | Self::SinglePane => LayoutMode::Narrow,
            Self::DualPane => LayoutMode::Normal,
            Self::DualCompactPreview
            | Self::DualPreview
            | Self::DeveloperWide
            | Self::UltraWide => LayoutMode::Wide,
        }
    }

    /// Whether this tier renders a preview region.
    pub const fn has_preview(self) -> bool {
        matches!(
            self,
            Self::DualCompactPreview | Self::DualPreview | Self::DeveloperWide | Self::UltraWide
        )
    }

    /// Whether this tier renders dual file panes.
    pub const fn has_dual_panes(self) -> bool {
        !matches!(self, Self::Emergency | Self::Compact | Self::SinglePane)
    }
}

/// How the main content is divided into visual regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// One pane, when the terminal is too narrow for more.
    Narrow,
    /// Two panes.
    Normal,
    /// Two panes and a preview region.
    Wide,
}

impl LayoutMode {
    /// The mode a terminal of `width` columns uses with standard height.
    pub const fn for_width(width: u16) -> Self {
        if width < TWO_PANE_MIN_WIDTH {
            Self::Narrow
        } else if width < PREVIEW_MIN_WIDTH {
            Self::Normal
        } else {
            Self::Wide
        }
    }

    /// Determines the mode from the full `area` (width and height).
    pub const fn for_area(area: Rect) -> Self {
        ResponsiveTier::calculate(area).layout_mode()
    }

    /// How many regions the main content is divided into.
    pub const fn region_count(self) -> usize {
        match self {
            Self::Narrow => 1,
            Self::Normal => 2,
            Self::Wide => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_breakpoints_are_the_documented_widths() {
        assert_eq!(BREAKPOINT_COMPACT, 60);
        assert_eq!(BREAKPOINT_DUAL_PANE, 90);
        assert_eq!(BREAKPOINT_DUAL_COMPACT_PREVIEW, 120);
        assert_eq!(BREAKPOINT_DUAL_PREVIEW, 160);
        assert_eq!(BREAKPOINT_DEVELOPER_WIDE, 220);
        assert_eq!(BREAKPOINT_ULTRA_WIDE, 300);
    }

    #[test]
    fn responsive_tiers_calculate_accurately_for_standard_height() {
        let h = 30;
        assert_eq!(
            ResponsiveTier::calculate(Rect::new(0, 0, 20, h)),
            ResponsiveTier::Emergency
        );
        assert_eq!(
            ResponsiveTier::calculate(Rect::new(0, 0, 50, h)),
            ResponsiveTier::Compact
        );
        assert_eq!(
            ResponsiveTier::calculate(Rect::new(0, 0, 80, h)),
            ResponsiveTier::SinglePane
        );
        assert_eq!(
            ResponsiveTier::calculate(Rect::new(0, 0, 100, h)),
            ResponsiveTier::DualPane
        );
        assert_eq!(
            ResponsiveTier::calculate(Rect::new(0, 0, 140, h)),
            ResponsiveTier::DualCompactPreview
        );
        assert_eq!(
            ResponsiveTier::calculate(Rect::new(0, 0, 180, h)),
            ResponsiveTier::DualPreview
        );
        assert_eq!(
            ResponsiveTier::calculate(Rect::new(0, 0, 250, h)),
            ResponsiveTier::DeveloperWide
        );
        assert_eq!(
            ResponsiveTier::calculate(Rect::new(0, 0, 350, h)),
            ResponsiveTier::UltraWide
        );
    }

    #[test]
    fn responsive_tiers_degrade_gracefully_on_short_heights() {
        // Height < 4 -> Emergency regardless of width
        assert_eq!(
            ResponsiveTier::calculate(Rect::new(0, 0, 180, 2)),
            ResponsiveTier::Emergency
        );
        assert_eq!(
            ResponsiveTier::calculate(Rect::new(0, 0, 180, 3)),
            ResponsiveTier::Emergency
        );

        // Height 4..6 -> Degrades preview to DualPane on medium widths
        assert_eq!(
            ResponsiveTier::calculate(Rect::new(0, 0, 140, 5)),
            ResponsiveTier::DualPane
        );
        assert_eq!(
            ResponsiveTier::calculate(Rect::new(0, 0, 180, 5)),
            ResponsiveTier::DualPane
        );
    }

    #[test]
    fn tier_properties_reflect_layout_capabilities() {
        assert!(!ResponsiveTier::Emergency.has_dual_panes());
        assert!(!ResponsiveTier::Emergency.has_preview());

        assert!(!ResponsiveTier::SinglePane.has_dual_panes());
        assert!(!ResponsiveTier::SinglePane.has_preview());

        assert!(ResponsiveTier::DualPane.has_dual_panes());
        assert!(!ResponsiveTier::DualPane.has_preview());

        assert!(ResponsiveTier::DualCompactPreview.has_dual_panes());
        assert!(ResponsiveTier::DualCompactPreview.has_preview());

        assert!(ResponsiveTier::DualPreview.has_dual_panes());
        assert!(ResponsiveTier::DualPreview.has_preview());

        assert!(ResponsiveTier::DeveloperWide.has_dual_panes());
        assert!(ResponsiveTier::DeveloperWide.has_preview());

        assert!(ResponsiveTier::UltraWide.has_dual_panes());
        assert!(ResponsiveTier::UltraWide.has_preview());
    }

    #[test]
    fn layout_mode_region_counts() {
        assert_eq!(LayoutMode::Narrow.region_count(), 1);
        assert_eq!(LayoutMode::Normal.region_count(), 2);
        assert_eq!(LayoutMode::Wide.region_count(), 3);
    }
}
