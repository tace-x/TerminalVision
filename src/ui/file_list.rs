//! The file list renderer.
//!
//! Renders the visible entries of a [`Pane`] as rows inside the inner area
//! of a pane block. Each row shows a type indicator and the entry name,
//! with the selected row highlighted structurally and visually.
//!
//! The renderer reads only prepared application state. It never calls
//! `std::fs` or `Entry::metadata` and never mutates the pane.

use std::ffi::OsStr;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::state::Pane;
use crate::filesystem::entry::EntryKind;
use crate::git::FileStatus;
use crate::ui::theme::Theme;
use crate::ui::{display_width, truncate_filename_to_width, truncate_to_width};

/// Converts an `OsStr` name to a display string, truncated to `max_width` preserving extension.
fn name_to_display(name: &OsStr, max_width: usize) -> String {
    let lossy = name.to_string_lossy();
    truncate_filename_to_width(&lossy, max_width)
}

/// Renders the file list inside `area` for the given `pane`.
///
/// `is_active` controls whether the selected row uses the active (brighter)
/// or inactive (dimmer) highlight style.
pub fn render(frame: &mut Frame, area: Rect, pane: &Pane, is_active: bool) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let total = pane.visible_count();

    // Empty state.
    if total == 0 {
        render_empty(frame, area, pane);
        return;
    }

    // Walk-results mode: the pane is showing paths from a recursive search.
    if !pane.visible_results().is_empty() {
        render_results(frame, area, pane, is_active);
        return;
    }

    // Listing mode: the pane is showing its directory entries.
    render_listing(frame, area, pane, is_active);

    // Scroll indicators.
    render_scroll_indicators(frame, area, pane);
}

/// Renders the listing-mode file list.
fn render_listing(frame: &mut Frame, area: Rect, pane: &Pane, is_active: bool) {
    let height = area.height as usize;
    let width = area.width as usize;
    let scroll = pane.scroll_offset();
    let selected = pane.selected_index();
    let git_status = pane.git_status();

    let mut lines: Vec<Line<'_>> = Vec::with_capacity(height);

    for (row_index, entry) in pane.visible_entries().skip(scroll).take(height).enumerate() {
        let position = scroll + row_index;
        let is_cursor = selected == Some(position);
        let is_multi_selected = pane.is_item_selected(position);
        let file_status = git_status.file_status_for(std::path::Path::new(entry.name()));
        let line = entry_line(
            entry.kind(),
            entry.name(),
            file_status,
            width,
            is_cursor,
            is_multi_selected,
            is_active,
        );
        lines.push(line);
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Renders the walk-results mode file list.
fn render_results(frame: &mut Frame, area: Rect, pane: &Pane, is_active: bool) {
    let height = area.height as usize;
    let width = area.width as usize;
    let scroll = pane.scroll_offset();
    let selected = pane.selected_index();
    let results = pane.visible_results();

    let mut lines: Vec<Line<'_>> = Vec::with_capacity(height);

    for (row_index, result) in results.iter().skip(scroll).take(height).enumerate() {
        let position = scroll + row_index;
        let is_cursor = selected == Some(position);
        let is_multi_selected = pane.is_item_selected(position);

        let relative = result.relative();
        let display_text = relative.display().to_string();
        let line = result_line(
            &display_text,
            width,
            is_cursor,
            is_multi_selected,
            is_active,
        );
        lines.push(line);
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Builds one row [`Line`] for a directory-listing entry.
fn entry_line(
    kind: EntryKind,
    name: &OsStr,
    git_status: Option<FileStatus>,
    width: usize,
    is_cursor: bool,
    is_multi_selected: bool,
    is_active: bool,
) -> Line<'static> {
    let theme = Theme::default();
    let name_str = name.to_string_lossy();
    let (icon_symbol, icon_style) = theme.file_icon_and_style(&name_str, kind);
    let icon_width = display_width(icon_symbol);

    let (marker, marker_style) = theme.row_marker(is_cursor, is_multi_selected, is_active);
    let marker_width = display_width(marker);

    let (status_str, status_width, status_style) = match git_status {
        Some(status) => {
            let (sym, st) = theme.git_file_status(status);
            (sym, display_width(sym), st)
        }
        None => ("", 0, Style::default()),
    };

    let show_status = status_width > 0 && width > marker_width + icon_width + status_width + 4;
    let effective_status_width = if show_status { status_width } else { 0 };

    // Available width for the name after marker, indicator, and optional Git status.
    let name_max = width.saturating_sub(marker_width + icon_width + effective_status_width);
    let name_text = name_to_display(name, name_max);

    let row_style = if is_cursor {
        theme.row_style(true, is_active)
    } else if is_multi_selected {
        theme.row_multi_selected
    } else {
        theme.row_unselected
    };

    let mut spans = vec![
        Span::styled(marker.to_string(), marker_style),
        Span::styled(
            icon_symbol.to_string(),
            if is_cursor { row_style } else { icon_style },
        ),
    ];

    if show_status {
        spans.push(Span::styled(
            status_str.to_string(),
            if is_cursor { row_style } else { status_style },
        ));
    }

    let text_style = if is_cursor {
        row_style
    } else if is_multi_selected {
        theme.row_multi_selected
    } else {
        icon_style
    };

    spans.push(Span::styled(name_text, text_style));

    Line::from(spans)
}

/// Builds one row [`Line`] for a walk-search result.
fn result_line(
    display_text: &str,
    width: usize,
    is_cursor: bool,
    is_multi_selected: bool,
    is_active: bool,
) -> Line<'static> {
    let theme = Theme::default();
    let (marker, marker_style) = theme.row_marker(is_cursor, is_multi_selected, is_active);
    let marker_width = display_width(marker);

    let text_max = width.saturating_sub(marker_width);
    let truncated = truncate_filename_to_width(display_text, text_max);

    let row_style = if is_cursor {
        theme.row_style(true, is_active)
    } else if is_multi_selected {
        theme.row_multi_selected
    } else {
        theme.row_unselected
    };

    let spans = vec![
        Span::styled(marker.to_string(), marker_style),
        Span::styled(truncated, row_style),
    ];

    Line::from(spans)
}

/// Returns the style for a row given its selection and focus state.
#[cfg(test)]
fn row_style(is_selected: bool, is_active: bool) -> Style {
    Theme::default().row_style(is_selected, is_active)
}

/// Renders the empty-state message centred in `area`.
fn render_empty(frame: &mut Frame, area: Rect, pane: &Pane) {
    let theme = Theme::default();
    let message = if pane.search().is_active() {
        "No matches found"
    } else {
        "Empty directory"
    };

    let width = area.width as usize;
    let text = truncate_to_width(message, width);

    // Centre vertically: place the message at the middle row.
    let vertical_offset = area.height / 2;
    let message_area = Rect {
        x: area.x,
        y: area.y.saturating_add(vertical_offset),
        width: area.width,
        height: 1.min(area.height.saturating_sub(vertical_offset)),
    };

    if message_area.height == 0 {
        return;
    }

    // Centre horizontally with padding.
    let text_width = display_width(&text);
    let padding = (width.saturating_sub(text_width)) / 2;
    let pad_str = " ".repeat(padding);

    let line = Line::from(vec![
        Span::raw(pad_str),
        Span::styled(text, theme.empty_state_text),
    ]);

    let paragraph = Paragraph::new(vec![line]);
    frame.render_widget(paragraph, message_area);
}

/// Renders scroll direction indicators when the listing extends beyond the
/// visible window.
///
/// An `↑` appears at the right edge of the first row when entries are
/// scrolled past above, and a `↓` at the right edge of the last row when
/// entries remain below.
fn render_scroll_indicators(frame: &mut Frame, area: Rect, pane: &Pane) {
    let theme = Theme::default();
    let height = area.height as usize;
    let total = pane.visible_count();
    let scroll = pane.scroll_offset();

    if height == 0 || area.width == 0 {
        return;
    }

    let has_above = scroll > 0;
    let has_below = scroll + height < total;

    if has_above {
        let indicator_area = Rect {
            x: area.x.saturating_add(area.width.saturating_sub(1)),
            y: area.y,
            width: 1,
            height: 1,
        };
        let indicator = Paragraph::new(Span::styled(theme.symbols.scroll_up, theme.header_path));
        frame.render_widget(indicator, indicator_area);
    }

    if has_below {
        let last_y = area.y.saturating_add(area.height.saturating_sub(1));
        let indicator_area = Rect {
            x: area.x.saturating_add(area.width.saturating_sub(1)),
            y: last_y,
            width: 1,
            height: 1,
        };
        let indicator = Paragraph::new(Span::styled(theme.symbols.scroll_down, theme.header_path));
        frame.render_widget(indicator, indicator_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::{ActivePane, App};
    use crate::filesystem::entry::EntryKind;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Modifier;

    #[test]
    fn empty_pane_renders_empty_directory_message() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();
        let pane = app.pane(ActivePane::Left);

        terminal
            .draw(|f| {
                render(f, Rect::new(0, 0, 40, 10), pane, true);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let all_text: String = (0..10)
            .flat_map(|y| (0..40).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(
            all_text.contains("Empty directory"),
            "empty pane must show 'Empty directory'"
        );
    }

    #[test]
    fn kind_indicator_returns_correct_symbols() {
        let theme = Theme::default();
        let (dir_sym, _) = theme.entry_kind_indicator(EntryKind::Directory);
        assert!(dir_sym.contains('📁'), "Directory should show folder emoji");

        let (file_sym, _) = theme.entry_kind_indicator(EntryKind::File);
        assert!(file_sym.starts_with('-'), "File should show dash");

        let (sym_sym, _) = theme.entry_kind_indicator(EntryKind::Symlink);
        assert!(sym_sym.starts_with('@'), "Symlink should show @");

        let (other_sym, _) = theme.entry_kind_indicator(EntryKind::Other);
        assert!(other_sym.starts_with('?'), "Other should show ?");
    }

    #[test]
    fn entry_line_selected_has_marker() {
        let line = entry_line(
            EntryKind::File,
            OsStr::new("test.txt"),
            None,
            40,
            true,
            false,
            true,
        );
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.starts_with("> "), "selected row must start with '> '");
    }

    #[test]
    fn entry_line_multi_selected_has_marker() {
        let line = entry_line(
            EntryKind::File,
            OsStr::new("test.txt"),
            None,
            40,
            false,
            true,
            true,
        );
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(
            text.starts_with(" ✓"),
            "multi-selected row must start with ' ✓'"
        );
    }

    #[test]
    fn entry_line_unselected_has_no_marker() {
        let line = entry_line(
            EntryKind::File,
            OsStr::new("test.txt"),
            None,
            40,
            false,
            false,
            true,
        );
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(
            text.starts_with("  "),
            "unselected row must start with spaces"
        );
    }

    #[test]
    fn entry_line_truncates_long_names() {
        let long_name = "a".repeat(200) + ".rs";
        let line = entry_line(
            EntryKind::File,
            OsStr::new(&long_name),
            None,
            20,
            false,
            false,
            true,
        );
        let _text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        let total_width: usize = line.spans.iter().map(|s| display_width(&s.content)).sum();
        assert!(
            total_width <= 20,
            "row display width {} must not exceed 20",
            total_width
        );
    }

    #[test]
    fn entry_line_handles_unicode_names() {
        let line = entry_line(
            EntryKind::File,
            OsStr::new("മലയാളം_ഫയൽ.txt"),
            None,
            20,
            false,
            false,
            true,
        );
        let total_width: usize = line.spans.iter().map(|s| display_width(&s.content)).sum();
        assert!(
            total_width <= 20,
            "unicode row display width {} must not exceed 20",
            total_width
        );
    }

    #[test]
    fn entry_line_renders_git_status_marker_when_space_permits() {
        let line = entry_line(
            EntryKind::File,
            OsStr::new("modified.txt"),
            Some(FileStatus::Modified),
            40,
            false,
            false,
            true,
        );
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(
            text.contains("M "),
            "line should contain modified status marker 'M '"
        );
    }

    #[test]
    fn row_style_active_selected_is_bold() {
        let style = row_style(true, true);
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn row_style_inactive_selected_is_not_bold() {
        let style = row_style(true, false);
        assert!(!style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn row_style_unselected_is_default() {
        let style = row_style(false, true);
        assert_eq!(style, Style::default());
    }

    #[test]
    fn render_handles_zero_area_without_panic() {
        let backend = TestBackend::new(1, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();
        let pane = app.pane(ActivePane::Left);

        // Zero width
        terminal
            .draw(|f| {
                render(f, Rect::new(0, 0, 0, 10), pane, true);
            })
            .unwrap();

        // Zero height
        terminal
            .draw(|f| {
                render(f, Rect::new(0, 0, 10, 0), pane, true);
            })
            .unwrap();
    }

    #[test]
    fn render_tiny_dimensions_do_not_panic() {
        let sizes = [(1, 1), (2, 1), (1, 2), (3, 2), (5, 3)];
        let app = App::default();
        let pane = app.pane(ActivePane::Left);

        for (w, h) in sizes {
            let backend = TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|f| {
                    render(f, Rect::new(0, 0, w, h), pane, true);
                })
                .unwrap();
        }
    }

    #[test]
    fn name_to_display_truncates_correctly() {
        let name = OsStr::new("very_long_filename_that_should_be_truncated.rs");
        let display = name_to_display(name, 10);
        assert!(display_width(&display) <= 10);
    }

    #[test]
    fn name_to_display_preserves_short_names() {
        let name = OsStr::new("short.rs");
        let display = name_to_display(name, 20);
        assert_eq!(display, "short.rs");
    }

    #[test]
    fn result_line_selected_has_marker() {
        let line = result_line("path/to/file.txt", 40, true, false, true);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(
            text.starts_with("> "),
            "selected result row must start with '> '"
        );
    }

    #[test]
    fn result_line_truncates_long_paths() {
        let long_path = "a/".repeat(100) + "file.txt";
        let line = result_line(&long_path, 20, false, false, true);
        let total_width: usize = line.spans.iter().map(|s| display_width(&s.content)).sum();
        assert!(
            total_width <= 20,
            "result row display width {} must not exceed 20",
            total_width
        );
    }

    #[test]
    fn test_phase17_3_responsive_file_row_tiers() {
        let name = OsStr::new("important_document.pdf");

        // Extreme width (15 cols)
        let line_extreme = entry_line(EntryKind::File, name, None, 15, true, false, true);
        let width_extreme: usize = line_extreme
            .spans
            .iter()
            .map(|s| display_width(&s.content))
            .sum();
        assert!(width_extreme <= 15);

        // Narrow width (30 cols)
        let line_narrow = entry_line(
            EntryKind::File,
            name,
            Some(FileStatus::Modified),
            30,
            false,
            true,
            true,
        );
        let width_narrow: usize = line_narrow
            .spans
            .iter()
            .map(|s| display_width(&s.content))
            .sum();
        assert!(width_narrow <= 30);

        // Medium width (60 cols)
        let line_med = entry_line(
            EntryKind::File,
            name,
            Some(FileStatus::Untracked),
            60,
            true,
            false,
            true,
        );
        let width_med: usize = line_med
            .spans
            .iter()
            .map(|s| display_width(&s.content))
            .sum();
        assert!(width_med <= 60);

        // Wide width (120 cols)
        let line_wide = entry_line(
            EntryKind::File,
            name,
            Some(FileStatus::Added),
            120,
            false,
            false,
            false,
        );
        let width_wide: usize = line_wide
            .spans
            .iter()
            .map(|s| display_width(&s.content))
            .sum();
        assert!(width_wide <= 120);
    }

    #[test]
    fn test_phase17_3_symlink_rendering_distinction() {
        let symlink_line = entry_line(
            EntryKind::Symlink,
            OsStr::new("symlink_target"),
            None,
            40,
            false,
            false,
            true,
        );
        let text: String = symlink_line
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains('@') || text.contains("->"));
    }

    #[test]
    fn test_phase17_3_git_status_all_variants_render() {
        let statuses = [
            (FileStatus::Modified, "M"),
            (FileStatus::Added, "A"),
            (FileStatus::Deleted, "D"),
            (FileStatus::Renamed, "R"),
            (FileStatus::Untracked, "?"),
        ];

        for (status, marker) in statuses {
            let line = entry_line(
                EntryKind::File,
                OsStr::new("file.rs"),
                Some(status),
                50,
                false,
                false,
                true,
            );
            let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
            assert!(
                text.contains(marker),
                "Line should contain git marker '{marker}' for status {:?}",
                status
            );
        }
    }
}
