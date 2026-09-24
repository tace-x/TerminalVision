//! The file panes.
//!
//! Renders the responsive pane regions (Single, Two, or Three) produced by the
//! layout engine. Each pane displays a bordered block whose title shows the
//! current directory path, and whose content is the file list rendered by
//! [`file_list::render`].

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};

use crate::app::state::{ActivePane, App, Pane};
use crate::layout::geometry::MainLayout;
use crate::ui::preview;
use crate::ui::theme::Theme;
use crate::ui::{display_width, file_list, truncate_path_to_width, truncate_to_width};

/// Renders the main content area according to the responsive `MainLayout`.
pub fn render(frame: &mut Frame, layout: MainLayout, app: &App) {
    if app.is_focus_mode() {
        let active = app.active_pane();
        let total_area = match layout {
            MainLayout::Single { pane } => pane,
            MainLayout::Two { left, right } => Rect::new(
                left.x,
                left.y,
                left.width.saturating_add(right.width),
                left.height,
            ),
            MainLayout::Three {
                left,
                right,
                preview,
            } => Rect::new(
                left.x,
                left.y,
                left.width
                    .saturating_add(right.width)
                    .saturating_add(preview.width),
                left.height,
            ),
        };
        render_pane(frame, total_area, app, active, true);
        return;
    }

    match layout {
        MainLayout::Single { pane } => {
            let active = app.active_pane();
            render_pane(frame, pane, app, active, true);
        }
        MainLayout::Two { left, right } => {
            let active = app.active_pane();
            render_pane(
                frame,
                left,
                app,
                ActivePane::Left,
                active == ActivePane::Left,
            );
            render_pane(
                frame,
                right,
                app,
                ActivePane::Right,
                active == ActivePane::Right,
            );
        }
        MainLayout::Three {
            left,
            right,
            preview,
        } => {
            let active = app.active_pane();
            render_pane(
                frame,
                left,
                app,
                ActivePane::Left,
                active == ActivePane::Left,
            );
            render_pane(
                frame,
                right,
                app,
                ActivePane::Right,
                active == ActivePane::Right,
            );
            preview::render(frame, preview, app);
        }
    }
}

/// Builds the title Line and hit ranges for a pane's header.
///
/// Returns `(Line, hit_ranges)` where each hit range is `(tab_index, relative_start_x, relative_end_x)`.
pub fn build_pane_title(
    area_width: u16,
    pane: &Pane,
    is_active: bool,
) -> (Line<'static>, Vec<(usize, u16, u16)>) {
    if area_width < 4 {
        return (Line::default(), Vec::new());
    }

    let theme = Theme::default();
    let tabs = pane.tabs();
    let active_tab_idx = pane.active_tab_index();

    if tabs.len() <= 1 {
        let path = pane.current_path();
        let max_path_width = (area_width as usize).saturating_sub(if is_active { 7 } else { 6 });
        let path_display = truncate_path_to_width(path, max_path_width);
        let (raw_text, title_style) = if is_active {
            (
                format!(" {} {path_display} ", theme.symbols.active_indicator),
                theme.active_pane_title,
            )
        } else {
            (format!("   {path_display} "), theme.inactive_pane_title)
        };
        let title_text = truncate_to_width(&raw_text, area_width as usize);
        let line = Line::from(Span::styled(title_text, title_style));
        let hit_ranges = vec![(0, 0, area_width)];
        return (line, hit_ranges);
    }

    // Multiple tabs:
    let avail_width = (area_width as usize).saturating_sub(2);
    let tab_count = tabs.len();

    // On narrow widths where multiple tabs cannot fit comfortably, render only the active tab
    let min_needed_all_tabs = tab_count.saturating_mul(8);
    if avail_width < min_needed_all_tabs {
        let active_tab = pane.active_tab();
        let num_prefix = format!("{}: ", active_tab_idx + 1);
        let dot = if is_active {
            format!("{} ", theme.symbols.active_indicator)
        } else {
            format!("{} ", theme.symbols.inactive_indicator)
        };
        let prefix_width = display_width(&dot) + display_width(&num_prefix) + 2; // for brackets
        let text = if avail_width <= prefix_width {
            let raw = format!("[{}{}]", dot, num_prefix.trim_end());
            truncate_to_width(&raw, avail_width)
        } else {
            let name_budget = avail_width.saturating_sub(prefix_width);
            let truncated_name = truncate_to_width(&active_tab.name(), name_budget);
            format!("[{}{}{}]", dot, num_prefix, truncated_name)
        };
        let style = if is_active {
            theme.tab_active_focused
        } else {
            theme.tab_active_unfocused
        };
        let hit_ranges = vec![(active_tab_idx, 0, area_width)];
        return (Line::from(Span::styled(text, style)), hit_ranges);
    }

    let separator_str = theme.symbols.vertical_separator;
    let sep_width = display_width(separator_str);
    let total_sep_width = sep_width * tab_count.saturating_sub(1);
    let avail_for_tabs = avail_width.saturating_sub(total_sep_width);
    let per_tab_budget = avail_for_tabs / tab_count;

    let mut spans = Vec::new();
    let mut hit_ranges = Vec::new();
    let mut current_x = 1u16;

    for (idx, tab) in tabs.iter().enumerate() {
        if idx > 0 {
            let sep_span = Span::styled(separator_str, theme.tab_separator);
            spans.push(sep_span);
            current_x = current_x.saturating_add(sep_width as u16);
        }

        let is_selected_tab = idx == active_tab_idx;
        let tab_name = tab.name();
        let num_prefix = format!("{}: ", idx + 1);

        let (tab_text, tab_style) = if is_selected_tab {
            let dot = if is_active {
                format!("{} ", theme.symbols.active_indicator)
            } else {
                format!("{} ", theme.symbols.inactive_indicator)
            };
            let prefix_w = display_width(&dot) + display_width(&num_prefix) + 2;
            let name_budget = per_tab_budget.saturating_sub(prefix_w);
            let truncated_name = truncate_to_width(&tab_name, name_budget.max(1));
            let text = format!("[{}{}{}]", dot, num_prefix, truncated_name);
            let style = if is_active {
                theme.tab_active_focused
            } else {
                theme.tab_active_unfocused
            };
            (text, style)
        } else {
            let prefix_w = display_width(&num_prefix) + 2;
            let name_budget = per_tab_budget.saturating_sub(prefix_w);
            let truncated_name = truncate_to_width(&tab_name, name_budget.max(1));
            let text = format!(" {}{} ", num_prefix, truncated_name);
            let style = theme.tab_inactive;
            (text, style)
        };

        let width = display_width(&tab_text) as u16;
        let start_x = current_x;
        let end_x = current_x.saturating_add(width);
        hit_ranges.push((idx, start_x, end_x));
        current_x = end_x;

        spans.push(Span::styled(tab_text, tab_style));
    }

    (Line::from(spans), hit_ranges)
}

/// Returns the hit ranges for tabs on the top border of `pane_rect`.
pub fn tab_hit_ranges(pane_rect: Rect, pane: &Pane, is_active: bool) -> Vec<(usize, u16, u16)> {
    let (_, ranges) = build_pane_title(pane_rect.width, pane, is_active);
    ranges
        .into_iter()
        .map(|(idx, rel_start, rel_end)| {
            (
                idx,
                pane_rect.x.saturating_add(rel_start),
                pane_rect.x.saturating_add(rel_end),
            )
        })
        .collect()
}

/// Renders a single pane within `area`.
fn render_pane(frame: &mut Frame, area: Rect, app: &App, which: ActivePane, is_active: bool) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let theme = Theme::default();
    let pane = app.pane(which);

    let (border_type, border_style) = theme.pane_border(is_active);
    let (title_line, _) = build_pane_title(area.width, pane, is_active);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(border_style)
        .title(title_line);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    file_list::render(frame, inner, pane, is_active);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    #[test]
    fn single_layout_renders_active_pane() {
        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();
        let layout = MainLayout::Single {
            pane: Rect::new(0, 0, 80, 22),
        };

        terminal
            .draw(|f| {
                render(f, layout, &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let top_row: String = (0..80).map(|x| buffer[(x, 0)].symbol()).collect();
        // Active pane title starts with "●"
        assert!(
            top_row.contains("●"),
            "active pane must show ● indicator, got: {top_row}"
        );
    }

    #[test]
    fn two_layout_renders_both_panes_with_active_distinction() {
        let backend = TestBackend::new(120, 28);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();
        let layout = MainLayout::Two {
            left: Rect::new(0, 0, 60, 28),
            right: Rect::new(60, 0, 60, 28),
        };

        terminal
            .draw(|f| {
                render(f, layout, &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let top_row: String = (0..120).map(|x| buffer[(x, 0)].symbol()).collect();
        // Left should have Active indicator
        assert!(top_row.contains("●"), "active pane must show ● indicator");
    }

    #[test]
    fn three_layout_renders_panes_and_preview() {
        let backend = TestBackend::new(180, 38);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();
        let layout = MainLayout::Three {
            left: Rect::new(0, 0, 60, 38),
            right: Rect::new(60, 0, 60, 38),
            preview: Rect::new(120, 0, 60, 38),
        };

        terminal
            .draw(|f| {
                render(f, layout, &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let top_row: String = (0..180).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(top_row.contains("●"), "active left pane must show ●");
        assert!(top_row.contains("Preview"), "preview pane must render");
    }

    #[test]
    fn pane_renders_empty_directory_when_no_entries() {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();

        terminal
            .draw(|f| {
                render_pane(f, Rect::new(0, 0, 60, 20), &app, ActivePane::Left, true);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let all_text: String = (0..20)
            .flat_map(|y| (0..60).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(
            all_text.contains("Empty directory"),
            "empty pane must show 'Empty directory'"
        );
    }

    #[test]
    fn pane_zero_dimensions_do_not_panic() {
        let backend = TestBackend::new(1, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();

        // Zero width
        terminal
            .draw(|f| {
                render_pane(f, Rect::new(0, 0, 0, 10), &app, ActivePane::Left, true);
            })
            .unwrap();

        // Zero height
        terminal
            .draw(|f| {
                render_pane(f, Rect::new(0, 0, 10, 0), &app, ActivePane::Left, true);
            })
            .unwrap();
    }

    #[test]
    fn pane_tiny_dimensions_do_not_panic() {
        let sizes = [(1, 1), (2, 2), (3, 3), (5, 3), (10, 2)];
        let app = App::default();

        for (w, h) in sizes {
            let backend = TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|f| {
                    render_pane(f, Rect::new(0, 0, w, h), &app, ActivePane::Left, true);
                })
                .unwrap();
        }
    }

    #[test]
    fn test_multiple_tabs_render_in_pane_header() {
        let mut pane = Pane::default();
        pane.set_current_path(PathBuf::from("/home/user/src"));
        pane.new_tab(PathBuf::from("/home/user/docs"), Vec::new());
        pane.new_tab(PathBuf::from("/home/user/target"), Vec::new());
        pane.select_tab(0);

        let (line, ranges) = build_pane_title(80, &pane, true);
        assert_eq!(ranges.len(), 3);

        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("[● 1: src]"));
        assert!(text.contains("2: docs"));
        assert!(text.contains("3: target"));
        assert!(text.contains("│"));
    }

    #[test]
    fn test_tab_hit_ranges_correctness() {
        let mut pane = Pane::default();
        pane.set_current_path(PathBuf::from("/alpha"));
        pane.new_tab(PathBuf::from("/beta"), Vec::new());

        let pane_rect = Rect::new(10, 5, 60, 20);
        let ranges = tab_hit_ranges(pane_rect, &pane, true);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].0, 0);
        assert_eq!(ranges[1].0, 1);
        assert!(ranges[0].1 >= 10);
        assert!(ranges[0].2 > ranges[0].1);
        assert!(ranges[1].1 >= ranges[0].2);
    }

    #[test]
    fn test_tab_header_unicode_and_narrow_width_no_panic() {
        let mut pane = Pane::default();
        pane.set_current_path(PathBuf::from("/home/📁-projects-with-very-long-name"));
        pane.new_tab(PathBuf::from("/home/🦀-rust-workspace"), Vec::new());
        pane.new_tab(PathBuf::from("/home/日本語-directory"), Vec::new());

        for width in [5, 10, 15, 20, 30, 50, 80, 120] {
            let (line, _) = build_pane_title(width, &pane, true);
            let total_width: usize = line.spans.iter().map(|s| display_width(&s.content)).sum();
            // Header spans must fit within available bounds
            assert!(
                total_width <= (width as usize).max(4),
                "width {width} overflowed: total {total_width}"
            );
        }
    }
}
