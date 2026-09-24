//! The application header.
//!
//! Renders a compact, single-row header bar displaying the application name
//! and the active pane's current directory path.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::state::App;
use crate::ui::theme::Theme;
use crate::ui::{display_width, truncate_path_to_width, truncate_to_width};

/// Renders the compact header bar inside `area`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let theme = Theme::default();
    let width = area.width as usize;

    // Extremely constrained width: render minimal brand text.
    if width < 16 {
        let text = truncate_to_width("TerminalVision", width);
        let paragraph = Paragraph::new(text).style(theme.app_title);
        frame.render_widget(paragraph, area);
        return;
    }

    let title_span = Span::styled(" TerminalVision ", theme.app_title);
    let sep = Span::styled(
        format!("{} ", theme.symbols.vertical_separator),
        theme.header_separator,
    );

    let mut spans = vec![title_span];

    let project_info = app.project_info();
    let git_status = app.git_status();

    if let Some(summary) = project_info.summary_string(git_status) {
        let proj_text = format!("{summary} ");
        let proj_width = display_width(&proj_text);
        if proj_width + 20 <= width {
            let style = if git_status.is_clean {
                theme.project_info_clean
            } else {
                theme.project_info_dirty
            };
            spans.push(Span::styled(proj_text, style));
        } else if let Some(name) = project_info.name.as_deref() {
            let minimal_text = format!("[proj: {name}] ");
            if display_width(&minimal_text) + 16 <= width {
                spans.push(Span::styled(minimal_text, theme.tab_active_focused));
            }
        }
    } else if let Some(summary) = git_status.summary_string() {
        let git_text = format!("{summary} ");
        let git_width = display_width(&git_text);
        if git_width + 20 <= width {
            let style = if git_status.is_clean {
                theme.project_info_clean
            } else {
                theme.project_info_dirty
            };
            spans.push(Span::styled(git_text, style));
        } else if let Some(repo_name) = git_status.repository.repo_name() {
            let minimal_text = format!("[git: {repo_name}] ");
            if display_width(&minimal_text) + 16 <= width {
                spans.push(Span::styled(minimal_text, theme.tab_active_focused));
            }
        }
    }

    let current_width: usize = spans.iter().map(|s| display_width(&s.content)).sum();

    if width > current_width + 4 {
        let remaining = width - current_width - 2; // reserve 2 cells for "│ "
        let path = app.pane(app.active_pane()).current_path();
        let path_text = truncate_path_to_width(path, remaining);
        spans.push(sep);
        spans.push(Span::styled(path_text, theme.header_path));
    }

    let line = Line::from(spans);
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
    }

    #[test]
    fn header_handles_tiny_widths_without_panicking() {
        for width in [1, 5, 10, 15, 20] {
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
        let backend = TestBackend::new(50, 1);
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
        let content: String = (0..50).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(content.contains("TerminalVision"));
    }
}
