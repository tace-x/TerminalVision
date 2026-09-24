//! The file and code preview renderer.
//!
//! Renders the prepared preview content from [`PreviewState`] into the reserved
//! preview area on wide layouts or in Preview mode with line numbering and
//! lightweight deterministic syntax highlighting.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::app::state::App;
use crate::filesystem::entry::EntryKind;
use crate::preview::metadata::MetadataPreview;
use crate::preview::syntax::{TokenKind, tokenize_line};
use crate::preview::{Language, PreviewContent};
use crate::ui::theme::Theme;
use crate::ui::{display_width, truncate_path_to_width, truncate_to_width};

/// Renders the preview widget inside `area`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let theme = Theme::default();
    let preview = app.preview();
    let title = match (
        preview
            .path()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str()),
        preview.content(),
    ) {
        (Some(name), Some(PreviewContent::Text(text_preview)))
            if text_preview.language() != Language::PlainText =>
        {
            format!(" [Preview: {name} • {}] ", text_preview.language().name())
        }
        (Some(name), Some(PreviewContent::Metadata(meta))) => {
            format!(" [Preview: {name} • {}] ", meta.kind_display())
        }
        (Some(name), _) => format!(" [Preview: {name}] "),
        (None, _) => " [Preview] ".to_string(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(theme.preview_border)
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    match preview.content() {
        Some(PreviewContent::Text(text_preview)) => {
            render_text_preview(frame, inner, text_preview);
        }
        Some(PreviewContent::Metadata(metadata_preview)) => {
            render_metadata_preview(frame, inner, metadata_preview);
        }
        Some(PreviewContent::Directory) => {
            let message = Paragraph::new("[Directory]")
                .alignment(Alignment::Center)
                .style(theme.preview_muted);
            frame.render_widget(message, inner);
        }
        Some(PreviewContent::Empty) => {
            let message = Paragraph::new("[Empty file]")
                .alignment(Alignment::Center)
                .style(theme.preview_muted);
            frame.render_widget(message, inner);
        }
        Some(PreviewContent::UnsupportedExtension(ext)) => {
            let message = Paragraph::new(format!("[Unsupported format: .{ext}]"))
                .alignment(Alignment::Center)
                .style(theme.preview_muted);
            frame.render_widget(message, inner);
        }
        Some(PreviewContent::Binary) => {
            let message = Paragraph::new("[Binary file - not displayed]")
                .alignment(Alignment::Center)
                .style(theme.preview_muted);
            frame.render_widget(message, inner);
        }
        Some(PreviewContent::InvalidUtf8) => {
            let message = Paragraph::new("[Not valid UTF-8 text]")
                .alignment(Alignment::Center)
                .style(theme.preview_muted);
            frame.render_widget(message, inner);
        }
        Some(PreviewContent::Error(err)) => {
            let message = Paragraph::new(format!("[Error: {err}]"))
                .alignment(Alignment::Center)
                .style(theme.notify_error_text);
            frame.render_widget(message, inner);
        }
        None => {
            let placeholder = Paragraph::new("Preview")
                .alignment(Alignment::Center)
                .style(theme.preview_muted);
            frame.render_widget(placeholder, inner);
        }
    }
}

struct MetadataField {
    label: &'static str,
    badge: Option<(&'static str, Style)>,
    value: String,
    value_style: Style,
    is_path: bool,
}

/// Renders structured filesystem entry metadata in a responsive layout.
fn render_metadata_preview(frame: &mut Frame, area: Rect, meta: &MetadataPreview) {
    let theme = Theme::default();
    let mut fields: Vec<MetadataField> = Vec::new();

    let (badge_str, badge_style) = if meta.is_broken_symlink() {
        (
            "[BROKEN LINK]",
            theme.entry_broken_symlink.add_modifier(Modifier::BOLD),
        )
    } else {
        match meta.kind() {
            EntryKind::File => ("[FILE]", theme.tab_active_focused),
            EntryKind::Directory => ("[DIR]", theme.entry_directory.add_modifier(Modifier::BOLD)),
            EntryKind::Symlink => ("[LINK]", theme.entry_symlink.add_modifier(Modifier::BOLD)),
            EntryKind::Other => ("[OTHER]", theme.git_modified.add_modifier(Modifier::BOLD)),
        }
    };

    fields.push(MetadataField {
        label: "Type",
        badge: Some((badge_str, badge_style)),
        value: meta.kind_display().to_string(),
        value_style: theme.preview_metadata_value,
        is_path: false,
    });

    fields.push(MetadataField {
        label: "Name",
        badge: None,
        value: meta.name().to_string(),
        value_style: theme.preview_metadata_value.add_modifier(Modifier::BOLD),
        is_path: false,
    });

    if let Some(ext) = meta.extension() {
        fields.push(MetadataField {
            label: "Extension",
            badge: None,
            value: format!(".{ext}"),
            value_style: theme.header_path,
            is_path: false,
        });
    }

    if let Some(target) = meta.target_path() {
        fields.push(MetadataField {
            label: "Target",
            badge: None,
            value: target.display().to_string(),
            value_style: if meta.is_broken_symlink() {
                theme.entry_broken_symlink
            } else {
                theme.entry_symlink
            },
            is_path: true,
        });
    }

    if meta.kind() == EntryKind::Symlink {
        if let Some(t_size) = meta.target_size() {
            fields.push(MetadataField {
                label: "Target Size",
                badge: None,
                value: crate::preview::format_size(t_size),
                value_style: theme.header_path,
                is_path: false,
            });
        }

        fields.push(MetadataField {
            label: "Link Size",
            badge: None,
            value: meta.size_display(),
            value_style: theme.header_path,
            is_path: false,
        });
    } else {
        fields.push(MetadataField {
            label: "Size",
            badge: None,
            value: meta.size_display(),
            value_style: theme.header_path,
            is_path: false,
        });
    }

    fields.push(MetadataField {
        label: "Modified",
        badge: None,
        value: meta.modified_display(),
        value_style: theme.preview_metadata_value,
        is_path: false,
    });

    fields.push(MetadataField {
        label: "Created",
        badge: None,
        value: meta.created_display(),
        value_style: theme.preview_metadata_value,
        is_path: false,
    });

    fields.push(MetadataField {
        label: "Accessed",
        badge: None,
        value: meta.accessed_display(),
        value_style: theme.preview_metadata_value,
        is_path: false,
    });

    let max_label_width = fields
        .iter()
        .map(|f| display_width(f.label))
        .max()
        .unwrap_or(8);

    let max_rows = area.height as usize;
    let total_width = area.width as usize;

    let mut lines = Vec::new();

    for (idx, field) in fields.into_iter().enumerate() {
        if idx >= max_rows {
            break;
        }

        let mut spans = Vec::new();

        if total_width < 25 {
            // Highly compact mode for very narrow viewports
            let prefix = format!("{}: ", field.label);
            let prefix_w = display_width(&prefix);
            spans.push(Span::styled(
                prefix,
                theme.preview_line_number.add_modifier(Modifier::BOLD),
            ));

            let avail = total_width.saturating_sub(prefix_w);
            let val = if field.is_path {
                truncate_path_to_width(std::path::Path::new(&field.value), avail)
            } else {
                truncate_to_width(&field.value, avail)
            };
            spans.push(Span::styled(val, field.value_style));
        } else {
            // Standard aligned layout
            let label_col = format!("  {:<width$} │ ", field.label, width = max_label_width);
            let label_col_w = display_width(&label_col);
            spans.push(Span::styled(label_col, theme.preview_line_number));

            let mut avail = total_width.saturating_sub(label_col_w);

            if let Some((b_text, b_style)) = field.badge {
                let badge_with_space = format!("{b_text} ");
                let badge_w = display_width(&badge_with_space);
                if badge_w < avail {
                    spans.push(Span::styled(badge_with_space, b_style));
                    avail -= badge_w;
                }
            }

            let val = if field.is_path {
                truncate_path_to_width(std::path::Path::new(&field.value), avail)
            } else {
                truncate_to_width(&field.value, avail)
            };
            spans.push(Span::styled(val, field.value_style));
        }

        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Renders code or text lines with dynamic line numbers and syntax styling.
fn render_text_preview(frame: &mut Frame, area: Rect, preview: &crate::preview::TextPreview) {
    let theme = Theme::default();
    let visible_lines = area.height as usize;
    let inner_width = area.width as usize;
    let total_lines = preview.line_count();
    let num_width = preview.line_number_width();
    let show_line_numbers = inner_width > num_width + 4;
    let num_col_width = if show_line_numbers { num_width + 3 } else { 0 };
    let code_width = inner_width.saturating_sub(num_col_width);

    let mut lines = Vec::new();

    for (idx, line_str) in preview.lines().iter().enumerate() {
        if idx >= visible_lines {
            break;
        }

        // If it's the last visible line and content is truncated
        if idx + 1 == visible_lines && (preview.is_truncated() || total_lines > visible_lines) {
            lines.push(Line::from(vec![Span::styled(
                "[Preview truncated]",
                theme.notify_warning,
            )]));
            continue;
        }

        let mut spans = Vec::new();

        if show_line_numbers {
            let num_text = format!("{:>width$} │ ", idx + 1, width = num_width);
            spans.push(Span::styled(num_text, theme.preview_line_number));
        }

        let code_spans = format_code_spans(line_str, preview.language(), code_width);
        spans.extend(code_spans);

        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Tokenizes and styles a line of code, truncating safely to `max_width`.
fn format_code_spans(line_str: &str, language: Language, max_width: usize) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current_width = 0;
    let tokens = tokenize_line(line_str, language);

    for token in tokens {
        if current_width >= max_width {
            break;
        }

        let token_w = display_width(token.text);
        if current_width + token_w <= max_width {
            spans.push(Span::styled(
                token.text.to_string(),
                token_style(token.kind),
            ));
            current_width += token_w;
        } else {
            let remaining = max_width - current_width;
            let truncated = truncate_to_width(token.text, remaining);
            spans.push(Span::styled(truncated, token_style(token.kind)));
            break;
        }
    }

    spans
}

/// Maps token kinds to Ratatui styling.
fn token_style(kind: TokenKind) -> Style {
    match kind {
        TokenKind::Keyword => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        TokenKind::StringLiteral => Style::default().fg(Color::Green),
        TokenKind::Comment => Style::default().fg(Color::DarkGray),
        TokenKind::Number => Style::default().fg(Color::Yellow),
        TokenKind::Boolean => Style::default().fg(Color::LightYellow),
        TokenKind::TypeOrFunction => Style::default().fg(Color::Cyan),
        TokenKind::Punctuation | TokenKind::Plain => Style::default().fg(Color::White),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::test_support::TempDir;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::fs;

    #[test]
    fn preview_renders_without_panic() {
        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..20)
            .flat_map(|y| (0..40).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("Preview"));
    }

    #[test]
    fn preview_zero_sized_area_does_not_panic() {
        let backend = TestBackend::new(1, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();

        terminal
            .draw(|f| {
                render(f, Rect::new(0, 0, 0, 0), &app);
            })
            .unwrap();
    }

    #[test]
    fn preview_responsive_sizes_do_not_panic() {
        let sizes = [(5, 5), (10, 10), (20, 10), (40, 20), (80, 24)];
        let app = App::default();

        for (w, h) in sizes {
            let backend = TestBackend::new(w, h);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|f| {
                    render(f, f.area(), &app);
                })
                .unwrap();
        }
    }

    #[test]
    fn preview_renders_directory_metadata() {
        let temp = TempDir::new("preview-dir-meta");
        let dir = temp.path().join("test_dir");
        fs::create_dir_all(&dir).unwrap();

        let mut app = App::at(temp.path().to_path_buf()).unwrap();
        app.handle_action(crate::app::actions::Action::Preview);

        let backend = TestBackend::new(50, 15);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..15)
            .flat_map(|y| (0..50).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("DIR") || text.contains("Directory"));
        assert!(text.contains("test_dir"));
    }

    #[test]
    fn preview_renders_symlink_metadata() {
        let temp = TempDir::new("preview-sym-meta");
        let target = temp.path().join("target.txt");
        fs::write(&target, "content").unwrap();
        let symlink = temp.path().join("link.txt");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &symlink).unwrap();
        #[cfg(windows)]
        let _ = std::os::windows::fs::symlink_file(&target, &symlink);

        if symlink.exists() || symlink.is_symlink() {
            let mut app = App::at(temp.path().to_path_buf()).unwrap();
            app.handle_action(crate::app::actions::Action::Preview);

            let backend = TestBackend::new(50, 15);
            let mut terminal = Terminal::new(backend).unwrap();

            terminal
                .draw(|f| {
                    render(f, f.area(), &app);
                })
                .unwrap();

            let buffer = terminal.backend().buffer();
            let text: String = (0..15)
                .flat_map(|y| (0..50).map(move |x| buffer[(x, y)].symbol().to_string()))
                .collect();
            assert!(text.contains("LINK") || text.contains("Symlink"));
        }
    }

    #[test]
    fn preview_renders_code_with_line_numbers() {
        let temp = TempDir::new("preview-code");
        let file = temp.path().join("main.rs");
        fs::write(&file, "fn main() {\n    println!(\"Hello\");\n}\n").unwrap();

        let mut app = App::at(temp.path().to_path_buf()).unwrap();
        app.handle_action(crate::app::actions::Action::Preview);

        let backend = TestBackend::new(60, 15);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text: String = (0..15)
            .flat_map(|y| (0..60).map(move |x| buffer[(x, y)].symbol().to_string()))
            .collect();
        assert!(text.contains("1 │"));
        assert!(text.contains("fn main"));
    }
}
