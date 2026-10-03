//! The application header.
//!
//! Renders a compact, single-row header bar displaying the application name,
//! project/git context, the active pane's current directory path, and a discoverable
//! Command Center search button affordance.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::state::App;
use crate::input::platform::Platform;
use crate::ui::{display_width, truncate_to_width};

/// Formats the discoverable Command Center and Quick Switcher entry point text.
pub fn command_center_button_text(platform: Platform, max_width: usize) -> Option<String> {
    let k_shortcut = if platform.is_mac() { "⌘K" } else { "Ctrl+K" };
    let p_shortcut = if platform.is_mac() { "⌘P" } else { "Ctrl+P" };

    if max_width >= 36 {
        Some(format!("[ {k_shortcut} Commands • {p_shortcut} Switcher ]"))
    } else if max_width >= 20 {
        Some(format!("[ {k_shortcut} Commands ]"))
    } else if max_width >= 12 {
        Some(format!("[ {k_shortcut} ]"))
    } else {
        None
    }
}

/// Returns the `(start_x, end_x, y)` hit testing coordinate range for the Command Center button in the header.
pub fn command_center_hit_range(area: Rect, platform: Platform) -> Option<(u16, u16, u16)> {
    if area.height == 0 || area.width < 30 {
        return None;
    }

    let width = area.width as usize;
    let button_str = command_center_button_text(platform, width.saturating_sub(25))?;
    let btn_width = display_width(&button_str) as u16;

    if btn_width + 15 > area.width {
        return None;
    }

    let start_x = area.x + area.width.saturating_sub(btn_width + 1);
    let end_x = start_x + btn_width;
    let y = area.y;

    Some((start_x, end_x, y))
}

use crate::navigation::SmartBreadcrumb;
use std::path::PathBuf;

/// Returns the target directory if a breadcrumb segment was clicked in the header.
pub fn breadcrumb_hit_segment(
    area: Rect,
    app: &App,
    mouse_x: u16,
    mouse_y: u16,
) -> Option<PathBuf> {
    if area.height == 0 || area.width == 0 || mouse_y != area.y {
        return None;
    }

    let width = area.width as usize;
    if width < 16 {
        return None;
    }

    let theme = app.theme();
    let platform = Platform::current();
    let mut left_width: usize = display_width(" TerminalVision ");

    let project_info = app.project_info();
    let git_status = app.git_status();

    if let Some(summary) = project_info.summary_string(git_status) {
        let proj_text = format!("{summary} ");
        let proj_width = display_width(&proj_text);
        if proj_width + 25 <= width {
            left_width += proj_width;
        } else if let Some(name) = project_info.name.as_deref() {
            let minimal_text = format!("[proj: {name}] ");
            if display_width(&minimal_text) + 20 <= width {
                left_width += display_width(&minimal_text);
            }
        }
    } else if let Some(summary) = git_status.summary_string() {
        let git_text = format!("{summary} ");
        let git_width = display_width(&git_text);
        if git_width + 25 <= width {
            left_width += git_width;
        } else if let Some(repo_name) = git_status.repository.repo_name() {
            let minimal_text = format!("[git: {repo_name}] ");
            if display_width(&minimal_text) + 20 <= width {
                left_width += display_width(&minimal_text);
            }
        }
    }

    let button_text = command_center_button_text(platform, width.saturating_sub(40));
    let button_width = button_text.as_ref().map_or(0, |s| display_width(s) + 1);

    if width <= left_width + button_width + 6 {
        return None;
    }

    // Add separator width " › "
    let sep_str = format!(" {} ", theme.symbols.chevron);
    left_width += display_width(&sep_str);

    let path = app.pane(app.active_pane()).current_path();
    let breadcrumb = SmartBreadcrumb::from_path(path);

    let mut cur_x = area.x + (left_width as u16);

    for (i, seg) in breadcrumb.segments.iter().enumerate() {
        if i > 0 {
            cur_x += 3; // " › "
        }
        let seg_len = display_width(&seg.name) as u16;
        let end_x = cur_x + seg_len;

        if mouse_x >= cur_x && mouse_x < end_x {
            return Some(seg.path.clone());
        }

        cur_x = end_x;
    }

    None
}

/// Renders the compact header bar inside `area`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let theme = app.theme();
    let width = area.width as usize;
    let platform = Platform::current();

    // Extremely constrained width: render minimal brand text.
    if width < 16 {
        let text = truncate_to_width("TerminalVision", width);
        let paragraph = Paragraph::new(text).style(theme.app_title);
        frame.render_widget(paragraph, area);
        return;
    }

    let title_span = Span::styled(" TerminalVision ", theme.app_title);
    let sep = Span::styled(
        format!(" {} ", theme.symbols.chevron),
        theme.header_separator,
    );

    let mut left_spans = vec![title_span];

    let project_info = app.project_info();
    let git_status = app.git_status();

    if let Some(summary) = project_info.summary_string(git_status) {
        let proj_text = format!("{summary} ");
        let proj_width = display_width(&proj_text);
        if proj_width + 25 <= width {
            let style = if git_status.is_clean {
                theme.project_info_clean
            } else {
                theme.project_info_dirty
            };
            left_spans.push(Span::styled(proj_text, style));
        } else if let Some(name) = project_info.name.as_deref() {
            let minimal_text = format!("[proj: {name}] ");
            if display_width(&minimal_text) + 20 <= width {
                left_spans.push(Span::styled(minimal_text, theme.tab_active_focused));
            }
        }
    } else if let Some(summary) = git_status.summary_string() {
        let git_text = format!("{summary} ");
        let git_width = display_width(&git_text);
        if git_width + 25 <= width {
            let style = if git_status.is_clean {
                theme.project_info_clean
            } else {
                theme.project_info_dirty
            };
            left_spans.push(Span::styled(git_text, style));
        } else if let Some(repo_name) = git_status.repository.repo_name() {
            let minimal_text = format!("[git: {repo_name}] ");
            if display_width(&minimal_text) + 20 <= width {
                left_spans.push(Span::styled(minimal_text, theme.tab_active_focused));
            }
        }
    }

    let button_text = command_center_button_text(platform, width.saturating_sub(40));
    let button_width = button_text.as_ref().map_or(0, |s| display_width(s) + 1);

    let current_left_width: usize = left_spans.iter().map(|s| display_width(&s.content)).sum();

    if width > current_left_width + button_width + 6 {
        let remaining = width - current_left_width - button_width - 3;
        let path = app.pane(app.active_pane()).current_path();
        let breadcrumb = SmartBreadcrumb::from_path(path);

        left_spans.push(sep);

        // Check if full spaced hierarchy fits
        let full_hierarchy_len: usize = breadcrumb
            .segments
            .iter()
            .map(|s| display_width(&s.name))
            .sum::<usize>()
            + breadcrumb.segments.len().saturating_sub(1) * 3;

        if full_hierarchy_len <= remaining {
            for (i, seg) in breadcrumb.segments.iter().enumerate() {
                if i > 0 {
                    left_spans.push(Span::styled(
                        format!(" {} ", theme.symbols.chevron),
                        theme.header_separator,
                    ));
                }
                if seg.is_current {
                    left_spans.push(Span::styled(seg.name.clone(), theme.tab_active_focused));
                } else {
                    left_spans.push(Span::styled(seg.name.clone(), theme.header_path));
                }
            }
        } else {
            let path_text = breadcrumb.format_for_width(remaining);
            left_spans.push(Span::styled(path_text, theme.header_path));
        }
    }

    let left_used_width: usize = left_spans.iter().map(|s| display_width(&s.content)).sum();

    if let Some(btn_str) = button_text
        && width > left_used_width + display_width(&btn_str)
    {
        let padding_spaces = width.saturating_sub(left_used_width + display_width(&btn_str) + 1);
        if padding_spaces > 0 {
            left_spans.push(Span::raw(" ".repeat(padding_spaces)));
        }
        left_spans.push(Span::styled(btn_str, theme.palette_shortcut));
    }

    let line = Line::from(left_spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    #[test]
    fn header_renders_on_standard_terminal() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..80).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(content.contains("TerminalVision"));
        assert!(content.contains("Commands"));
    }

    #[test]
    fn header_handles_tiny_widths_without_panicking() {
        for width in [1, 5, 10, 15, 20, 30, 50, 80, 120] {
            let backend = TestBackend::new(width, 1);
            let mut terminal = Terminal::new(backend).unwrap();
            let app = App::default();

            terminal
                .draw(|f| {
                    render(f, f.area(), &app);
                })
                .unwrap();
        }
    }

    #[test]
    fn header_truncates_long_paths() {
        let backend = TestBackend::new(60, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        let long_path =
            PathBuf::from("/a/very/long/nested/directory/path/that/exceeds/the/screen/width");
        let _ = app.open_in(crate::app::state::ActivePane::Left, long_path);

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..60).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(content.contains("TerminalVision"));
    }

    #[test]
    fn command_center_hit_testing() {
        let area = Rect::new(0, 0, 100, 1);
        let hit = command_center_hit_range(area, Platform::Mac);
        assert!(hit.is_some());
        let (start, end, y) = hit.unwrap();
        assert_eq!(y, 0);
        assert!(start < end);
        assert!(end <= 100);
    }
}
