//! Terminal geometry and responsive layout derivations.
//!
//! The size of the terminal is whatever the operating system reports: it is
//! never assumed and never fixed. From that area this module derives every
//! region the interface will occupy, so that no coordinate in the project is
//! written by hand.

use ratatui::layout::Rect;

use super::responsive::{LayoutMode, ResponsiveTier};

/// Height of the header region, in rows.
pub const HEADER_HEIGHT: u16 = 1;

/// Height of the footer region, in rows.
pub const FOOTER_HEIGHT: u16 = 1;

/// The size of the terminal, in columns and rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    columns: u16,
    rows: u16,
}

impl TerminalSize {
    /// Records a reported size.
    pub const fn new(columns: u16, rows: u16) -> Self {
        Self { columns, rows }
    }

    /// The number of columns the terminal reports.
    pub const fn columns(&self) -> u16 {
        self.columns
    }

    /// The number of rows the terminal reports.
    pub const fn rows(&self) -> u16 {
        self.rows
    }

    /// The whole terminal as an area, starting at the top left corner.
    pub const fn area(self) -> Rect {
        Rect::new(0, 0, self.columns, self.rows)
    }
}

/// A region of the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// The bar across the top.
    Header,
    /// The file pane on the left.
    LeftPane,
    /// The file pane on the right.
    RightPane,
    /// The preview beside the panes, on wide terminals.
    Preview,
    /// The integrated interactive terminal panel.
    Terminal,
    /// The bar across the bottom.
    Footer,
}

/// How the main content between the header and the footer is divided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainLayout {
    /// One pane, on a narrow terminal.
    Single {
        /// The pane.
        pane: Rect,
    },
    /// Two panes.
    Two {
        /// The pane on the left.
        left: Rect,
        /// The pane on the right.
        right: Rect,
    },
    /// Two panes and a preview region.
    Three {
        /// The pane on the left.
        left: Rect,
        /// The pane on the right.
        right: Rect,
        /// The preview.
        preview: Rect,
    },
}

impl MainLayout {
    /// Divides `area` according to the responsive tier.
    pub fn of(area: Rect, tier: ResponsiveTier) -> Self {
        match tier {
            ResponsiveTier::Emergency | ResponsiveTier::Compact | ResponsiveTier::SinglePane => {
                Self::Single { pane: area }
            }

            ResponsiveTier::DualPane => {
                // The left pane takes the smaller half, so that the two widths
                // add up to the parent exactly on an odd width.
                let left_width = area.width / 2;
                let left = Rect::new(area.x, area.y, left_width, area.height);
                let right = Rect::new(
                    area.x.saturating_add(left_width),
                    area.y,
                    area.width.saturating_sub(left_width),
                    area.height,
                );

                Self::Two { left, right }
            }

            ResponsiveTier::DualCompactPreview => {
                // Compact preview takes ~28% of width (clamped safely)
                let preview_width = if area.width >= 4 {
                    ((u32::from(area.width) * 28) / 100) as u16
                } else {
                    0
                };
                let panes_width = area.width.saturating_sub(preview_width);
                let left_width = panes_width / 2;

                let left = Rect::new(area.x, area.y, left_width, area.height);
                let right = Rect::new(
                    area.x.saturating_add(left_width),
                    area.y,
                    panes_width.saturating_sub(left_width),
                    area.height,
                );
                let preview = Rect::new(
                    area.x.saturating_add(panes_width),
                    area.y,
                    preview_width,
                    area.height,
                );

                Self::Three {
                    left,
                    right,
                    preview,
                }
            }

            ResponsiveTier::DualPreview => {
                // Standard preview takes 1/3 of width; remaining 2/3 split evenly between panes.
                let preview_width = area.width / 3;
                let panes_width = area.width.saturating_sub(preview_width);
                let left_width = panes_width / 2;

                let left = Rect::new(area.x, area.y, left_width, area.height);
                let right = Rect::new(
                    area.x.saturating_add(left_width),
                    area.y,
                    panes_width.saturating_sub(left_width),
                    area.height,
                );
                let preview = Rect::new(
                    area.x.saturating_add(panes_width),
                    area.y,
                    preview_width,
                    area.height,
                );

                Self::Three {
                    left,
                    right,
                    preview,
                }
            }

            ResponsiveTier::DeveloperWide => {
                // Developer wide layout gives 35% preview
                let preview_width = ((u32::from(area.width) * 35) / 100) as u16;
                let panes_width = area.width.saturating_sub(preview_width);
                let left_width = panes_width / 2;

                let left = Rect::new(area.x, area.y, left_width, area.height);
                let right = Rect::new(
                    area.x.saturating_add(left_width),
                    area.y,
                    panes_width.saturating_sub(left_width),
                    area.height,
                );
                let preview = Rect::new(
                    area.x.saturating_add(panes_width),
                    area.y,
                    preview_width,
                    area.height,
                );

                Self::Three {
                    left,
                    right,
                    preview,
                }
            }

            ResponsiveTier::UltraWide => {
                // Ultra wide layout gives 38% preview
                let preview_width = ((u32::from(area.width) * 38) / 100) as u16;
                let panes_width = area.width.saturating_sub(preview_width);
                let left_width = panes_width / 2;

                let left = Rect::new(area.x, area.y, left_width, area.height);
                let right = Rect::new(
                    area.x.saturating_add(left_width),
                    area.y,
                    panes_width.saturating_sub(left_width),
                    area.height,
                );
                let preview = Rect::new(
                    area.x.saturating_add(panes_width),
                    area.y,
                    preview_width,
                    area.height,
                );

                Self::Three {
                    left,
                    right,
                    preview,
                }
            }
        }
    }

    /// The regions of the main content, from left to right.
    pub fn regions(&self) -> Vec<(Region, Rect)> {
        match *self {
            Self::Single { pane } => vec![(Region::LeftPane, pane)],
            Self::Two { left, right } => {
                vec![(Region::LeftPane, left), (Region::RightPane, right)]
            }
            Self::Three {
                left,
                right,
                preview,
            } => vec![
                (Region::LeftPane, left),
                (Region::RightPane, right),
                (Region::Preview, preview),
            ],
        }
    }

    /// The area the main content occupies.
    ///
    /// Every pane of the main content spans this area's height, and their widths
    /// add up to its width.
    pub fn area(&self) -> Rect {
        match *self {
            Self::Single { pane } => pane,
            Self::Two { left, .. } => Rect {
                width: self.regions_width(),
                ..left
            },
            Self::Three { left, .. } => Rect {
                width: self.regions_width(),
                ..left
            },
        }
    }

    /// The total width of the main content.
    fn regions_width(&self) -> u16 {
        self.regions().iter().map(|(_, rect)| rect.width).sum()
    }
}

/// Every region of the interface, derived from the terminal area.
///
/// The rectangles are clean: each one covers its whole region, including
/// whatever border a component later draws. The layout never subtracts border
/// widths, and a component must draw its border inside the rectangle it is
/// given rather than shrinking it. Adjacent regions therefore touch exactly,
/// which is what the geometry tests check.
///
/// The regions tile the area: the header, the main content and the footer fill
/// the height, and the panes fill the width of the main content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenLayout {
    header: Rect,
    main: MainLayout,
    terminal: Rect,
    footer: Rect,
    mode: LayoutMode,
    tier: ResponsiveTier,
}

impl ScreenLayout {
    /// Derives every region from the terminal area.
    ///
    /// The height is granted in order: the header, then the footer. The remaining
    /// height is responsively divided between the file manager panes and the
    /// integrated interactive terminal panel.
    pub fn calculate(area: Rect) -> Self {
        let tier = ResponsiveTier::calculate(area);
        let header_height = HEADER_HEIGHT.min(area.height);
        let footer_height = FOOTER_HEIGHT.min(area.height.saturating_sub(header_height));
        let avail_height = area
            .height
            .saturating_sub(header_height.saturating_add(footer_height));

        let (main_height, terminal_height) = if avail_height == 0 {
            (0, 0)
        } else if avail_height <= 6 {
            let term = (avail_height / 2).max(1);
            (avail_height.saturating_sub(term), term)
        } else {
            let max_term = avail_height.saturating_sub(3);
            let min_term = 3.min(max_term);
            let term = (((avail_height as u32 * 35) / 100) as u16).clamp(min_term, max_term);
            (avail_height.saturating_sub(term), term)
        };

        let header = Rect::new(area.x, area.y, area.width, header_height);

        let main_y = area.y.saturating_add(header_height);
        let main_area = Rect::new(area.x, main_y, area.width, main_height);

        let term_y = main_y.saturating_add(main_height);
        let terminal = Rect::new(area.x, term_y, area.width, terminal_height);

        let footer_y = term_y.saturating_add(terminal_height);
        let footer = Rect::new(area.x, footer_y, area.width, footer_height);

        Self {
            header,
            main: MainLayout::of(main_area, tier),
            terminal,
            footer,
            mode: tier.layout_mode(),
            tier,
        }
    }

    /// The header region.
    pub fn header(&self) -> Rect {
        self.header
    }

    /// The main content region, and how it is divided.
    pub fn main(&self) -> MainLayout {
        self.main
    }

    /// The integrated terminal region.
    pub fn terminal(&self) -> Rect {
        self.terminal
    }

    /// The footer region.
    pub fn footer(&self) -> Rect {
        self.footer
    }

    /// The width the terminal had when this layout was calculated.
    pub fn width(&self) -> u16 {
        self.header.width
    }

    /// The height the terminal had when this layout was calculated.
    pub fn height(&self) -> u16 {
        self.header.height + self.main.area().height + self.terminal.height + self.footer.height
    }

    /// The layout mode selected by the area.
    pub fn mode(&self) -> LayoutMode {
        self.mode
    }

    /// The detailed responsive tier calculated for this area.
    pub fn tier(&self) -> ResponsiveTier {
        self.tier
    }

    /// Every region of the interface, from top to bottom and left to right.
    pub fn regions(&self) -> Vec<(Region, Rect)> {
        let mut regions = Vec::with_capacity(3 + self.mode.region_count());
        regions.push((Region::Header, self.header));
        regions.extend(self.main.regions());
        regions.push((Region::Terminal, self.terminal));
        regions.push((Region::Footer, self.footer));
        regions
    }
}

#[cfg(test)]
mod tests {
    use super::{FOOTER_HEIGHT, HEADER_HEIGHT, Region, ScreenLayout, TerminalSize};
    use ratatui::layout::Rect;

    /// The complete extreme terminal size test matrix required by Phase 17.2.
    const EXTREME_TEST_MATRIX: [(u16, u16); 22] = [
        (1, 1),
        (10, 3),
        (20, 5),
        (30, 8),
        (40, 10),
        (50, 12),
        (60, 15),
        (70, 20),
        (80, 24),
        (90, 24),
        (100, 30),
        (110, 30),
        (120, 30),
        (140, 40),
        (159, 40),
        (160, 40),
        (180, 50),
        (200, 60),
        (220, 70),
        (240, 80),
        (300, 100),
        (400, 120),
    ];

    /// Odd dimensions that cannot be halved or thirded evenly.
    const ODD_DIMENSIONS: [(u16, u16); 6] = [
        (73, 17),
        (97, 23),
        (113, 29),
        (137, 37),
        (167, 43),
        (203, 61),
    ];

    /// Widths either side of each breakpoint, and on it.
    const BREAKPOINT_WIDTHS: [u16; 18] = [
        59, 60, 61, 89, 90, 91, 119, 120, 121, 159, 160, 161, 219, 220, 221, 299, 300, 301,
    ];

    /// A spread of heights, from cramped to roomy.
    const PLANNED_HEIGHTS: [u16; 8] = [10, 15, 20, 24, 30, 40, 60, 100];

    /// Sizes a user might drag a terminal through, in this order.
    const RESIZE_SEQUENCE: [(u16, u16); 9] = [
        (80, 24),
        (120, 30),
        (160, 40),
        (200, 60),
        (100, 20),
        (159, 30),
        (99, 15),
        (240, 100),
        (40, 10),
    ];

    fn area(width: u16, height: u16) -> Rect {
        TerminalSize::new(width, height).area()
    }

    /// A readable name for a region, used in failure messages.
    fn label(region: Region) -> &'static str {
        match region {
            Region::Header => "header",
            Region::LeftPane => "left pane",
            Region::RightPane => "right pane",
            Region::Preview => "preview",
            Region::Terminal => "terminal",
            Region::Footer => "footer",
        }
    }

    fn labelled(layout: &ScreenLayout) -> Vec<(&'static str, Rect)> {
        layout
            .regions()
            .into_iter()
            .map(|(region, rect)| (label(region), rect))
            .collect()
    }

    /// Every region must sit inside the area it was derived from.
    fn assert_rect_inside(root: Rect, child: Rect, label: &str) {
        assert!(
            child.left() >= root.left()
                && child.top() >= root.top()
                && child.right() <= root.right()
                && child.bottom() <= root.bottom(),
            "{label} {child:?} escaped the root {root:?}"
        );
    }

    /// Regions must not overlap unless that was intended, and none is.
    fn assert_no_overlap(regions: &[(&str, Rect)]) {
        for (index, (first_label, first)) in regions.iter().enumerate() {
            for (second_label, second) in &regions[index + 1..] {
                if first.is_empty() || second.is_empty() {
                    continue;
                }
                assert!(
                    !first.intersects(*second),
                    "{first_label} {first:?} overlaps {second_label} {second:?}"
                );
            }
        }
    }

    /// The split widths must add up to the width they were given.
    fn assert_width_conservation(regions: &[(&str, Rect)], available_width: u16) {
        let width: u32 = regions.iter().map(|(_, rect)| u32::from(rect.width)).sum();
        assert_eq!(
            width,
            u32::from(available_width),
            "the split widths must add up to the available width"
        );
    }

    /// Horizontally adjacent regions must touch and fill the width together.
    fn assert_horizontal_alignment(regions: &[(&str, Rect)], total_width: u16) {
        assert_width_conservation(regions, total_width);

        for pair in regions.windows(2) {
            let (left_label, left) = pair[0];
            let (right_label, right) = pair[1];
            assert_eq!(
                left.right(),
                right.left(),
                "{left_label} and {right_label} must touch"
            );
            assert_eq!(left.top(), right.top(), "the regions must share a top");
            assert_eq!(
                left.bottom(),
                right.bottom(),
                "the regions must share a bottom"
            );
        }
    }

    /// The stacked heights must add up to the height they were given.
    fn assert_height_conservation(area: Rect, layout: &ScreenLayout) {
        let header = layout.header().height;
        let main = layout.main().area().height;
        let terminal = layout.terminal().height;
        let footer = layout.footer().height;

        assert_eq!(
            u32::from(header) + u32::from(main) + u32::from(terminal) + u32::from(footer),
            u32::from(area.height),
            "the stacked heights must add up to the available height"
        );
        assert_eq!(header, HEADER_HEIGHT.min(area.height));
        assert_eq!(
            footer,
            FOOTER_HEIGHT.min(area.height.saturating_sub(header))
        );
    }

    /// Vertically stacked regions must touch and fill the height together.
    fn assert_vertical_alignment(regions: &[(&str, Rect)], total_height: u16) {
        let height: u32 = regions.iter().map(|(_, rect)| u32::from(rect.height)).sum();
        assert_eq!(
            height,
            u32::from(total_height),
            "the heights must fill the area"
        );

        for pair in regions.windows(2) {
            let (top_label, top) = pair[0];
            let (bottom_label, bottom) = pair[1];
            assert_eq!(
                top.bottom(),
                bottom.top(),
                "{top_label} and {bottom_label} must touch"
            );
            assert_eq!(top.left(), bottom.left(), "the regions must share a left");
            assert_eq!(
                top.right(),
                bottom.right(),
                "the regions must share a right"
            );
        }
    }

    /// Every invariant that must hold for any terminal size.
    fn assert_layout_invariants(area: Rect, layout: &ScreenLayout) {
        let regions = labelled(layout);
        let main_area = layout.main().area();

        for (label, rect) in &regions {
            assert_rect_inside(area, *rect, label);
        }

        assert_no_overlap(&regions);
        assert_height_conservation(area, layout);

        assert_vertical_alignment(
            &[
                ("header", layout.header()),
                ("main", main_area),
                ("terminal", layout.terminal()),
                ("footer", layout.footer()),
            ],
            area.height,
        );
        assert_eq!(layout.header().width, area.width);
        assert_eq!(main_area.width, area.width);
        assert_eq!(layout.terminal().width, area.width);
        assert_eq!(layout.footer().width, area.width);

        let main_regions = layout.main().regions();
        assert_eq!(
            main_regions.len(),
            layout.mode().region_count(),
            "the mode must decide how many regions there are"
        );
        assert_horizontal_alignment(
            &main_regions
                .iter()
                .map(|(region, rect)| (label(*region), *rect))
                .collect::<Vec<_>>(),
            main_area.width,
        );

        for (region, rect) in &main_regions {
            let name = label(*region);
            assert_rect_inside(main_area, *rect, name);
            assert_eq!(
                rect.height, main_area.height,
                "{name} must be as tall as the main content"
            );
            if area.width > 0 && area.height >= 3 {
                assert!(rect.width > 0, "{name} must not be zero columns wide");
            }
        }

        if area.height > 0 {
            assert!(layout.header().height > 0, "the header must be visible");
        }
        if area.height > 1 {
            assert!(layout.footer().height > 0, "the footer must be visible");
        }
    }

    fn layout_of(width: u16, height: u16) -> ScreenLayout {
        let area = area(width, height);
        let layout = ScreenLayout::calculate(area);
        assert_layout_invariants(area, &layout);
        layout
    }

    #[test]
    fn extreme_test_matrix_all_hold_invariants() {
        for (width, height) in EXTREME_TEST_MATRIX {
            layout_of(width, height);
        }
    }

    #[test]
    fn odd_dimensions_conserve_every_pixel() {
        for (width, height) in ODD_DIMENSIONS {
            let layout = layout_of(width, height);
            let main = layout.main();
            let regions: Vec<(&str, Rect)> = main
                .regions()
                .iter()
                .map(|(region, rect)| (label(*region), *rect))
                .collect();

            assert_width_conservation(&regions, width);
            assert_eq!(main.area().width, width);
            assert_no_overlap(&regions);
        }
    }

    #[test]
    fn breakpoint_boundary_testing() {
        for width in BREAKPOINT_WIDTHS {
            for height in PLANNED_HEIGHTS {
                let layout = layout_of(width, height);
                assert_eq!(layout.width(), width);
            }
        }
    }

    #[test]
    fn height_boundary_testing() {
        for height in 0..=12 {
            for width in [40, 80, 100, 140, 180, 240] {
                layout_of(width, height);
            }
        }
    }

    #[test]
    fn resize_simulation_deterministic() {
        for (width, height) in RESIZE_SEQUENCE {
            let area = area(width, height);
            let l1 = ScreenLayout::calculate(area);
            assert_layout_invariants(area, &l1);
            let l2 = ScreenLayout::calculate(area);
            assert_eq!(l1, l2);
        }
    }

    #[test]
    fn resize_large_to_small_to_large() {
        let large = area(240, 100);
        let small = area(40, 10);

        let large1 = ScreenLayout::calculate(large);
        let small1 = ScreenLayout::calculate(small);
        let large2 = ScreenLayout::calculate(large);

        assert_ne!(large1.tier(), small1.tier());
        assert_eq!(large1, large2, "resize back to large must be identical");
    }

    #[test]
    fn sweeps_up_to_four_hundred_columns() {
        for height in [0, 1, 2, 3, 5, 10, 24, 60] {
            for width in 0..=400 {
                layout_of(width, height);
            }
        }
    }
}
