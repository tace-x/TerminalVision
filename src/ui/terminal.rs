//! Integrated interactive terminal panel renderer.
//!
//! Renders the live ANSI terminal emulator buffer, interactive shell prompt,
//! active/inactive focus indicators, cursor position, and bounded scrollback.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::state::App;
use crate::ui::theme::Theme;
use crate::ui::truncate_path_to_width;

/// Renders the integrated terminal panel inside `area`.
pub fn render(frame: &mut Frame, area: Rect, app: &App, is_focused: bool) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let theme = Theme::default();
    let (border_type, border_style) = theme.terminal_border(is_focused);

    let shell_name = app.terminal_shell_name();
    let cwd = app.terminal_cwd();
    let max_path_len = (area.width as usize).saturating_sub(35).max(10);
    let path_display = truncate_path_to_width(&cwd, max_path_len);

    let title = if is_focused {
        Line::from(vec![
            Span::styled(" ⚡ TERMINAL ", theme.terminal_title_focused),
            Span::styled(
                format!("│ {shell_name} │ {path_display} "),
                theme.terminal_meta_focused,
            ),
            Span::styled("[Focused - Ctrl+T to switch]", theme.footer_hint),
        ])
    } else {
        Line::from(vec![
            Span::styled(" TERMINAL ", theme.terminal_title_unfocused),
            Span::styled(
                format!("│ {shell_name} │ {path_display} "),
                theme.terminal_meta_unfocused,
            ),
            Span::styled("[Ctrl+T / Click to focus]", theme.footer_hint),
        ])
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(border_style)
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let rows = app.terminal_visible_rows();
    let (cursor_row, cursor_col, cursor_visible) = app.terminal_cursor_info();
    let inner_height = inner.height as usize;
    let inner_width = inner.width as usize;

    let mut lines = Vec::with_capacity(inner_height);

    for (r_idx, row) in rows.into_iter().take(inner_height).enumerate() {
        let mut spans = Vec::new();
        for (c_idx, cell) in row.into_iter().take(inner_width).enumerate() {
            let mut style = cell.style;

            // Render focused cursor
            if is_focused
                && cursor_visible
                && r_idx == cursor_row as usize
                && c_idx == cursor_col as usize
            {
                style = style
                    .add_modifier(Modifier::REVERSED)
                    .bg(Color::Cyan)
                    .fg(Color::Black);
            }

            spans.push(Span::styled(cell.ch.to_string(), style));
        }

        lines.push(Line::from(spans));
    }

    // Fill remaining rows if buffer is smaller than inner area
    while lines.len() < inner_height {
        lines.push(Line::default());
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_terminal_panel_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();

        terminal
            .draw(|f| {
                let area = Rect::new(0, 14, 80, 8);
                render(f, area, &app, true);
                render(f, area, &app, false);
            })
            .unwrap();
    }
}
