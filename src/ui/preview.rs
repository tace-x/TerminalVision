//! The file and code preview renderer.
//!
//! Renders the prepared preview content from [`PreviewState`] into the reserved
//! preview area on wide layouts or in Quick Preview mode with line numbering,
//! syntax highlighting, image details, PDF metadata, archive tables, and directory summaries.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::app::state::App;
use crate::filesystem::entry::EntryKind;
use crate::preview::archive::ArchivePreview;
use crate::preview::directory::DirectoryPreview;
use crate::preview::image::ImagePreview;
use crate::preview::metadata::MetadataPreview;
use crate::preview::pdf::PdfPreview;
use crate::preview::syntax::{TokenKind, tokenize_line};
use crate::preview::{Language, PreviewContent};
use crate::ui::theme::Theme;
use crate::ui::{display_width, truncate_path_to_width, truncate_to_width};

/// Renders the preview widget inside `area`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let theme = app.theme();
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
        (Some(name), Some(PreviewContent::Image(img))) => {
            format!(" [Preview: {name} • {}] ", img.format.short_name())
        }
        (Some(name), Some(PreviewContent::Pdf(_))) => {
            format!(" [Preview: {name} • PDF] ")
        }
        (Some(name), Some(PreviewContent::Archive(arc))) => {
            format!(" [Preview: {name} • {}] ", arc.format)
        }
        (Some(name), Some(PreviewContent::Directory(_))) => {
            format!(" [Preview: {name} • Directory] ")
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

    render_preview_inner_with_theme(frame, inner, preview.content(), &theme);
}

/// Renders the inner content of a preview widget with the default theme.
pub fn render_preview_inner(frame: &mut Frame, inner: Rect, content: Option<&PreviewContent>) {
    render_preview_inner_with_theme(frame, inner, content, &Theme::default());
}

/// Renders the inner content of a preview widget given any `PreviewContent` and `Theme`.
pub fn render_preview_inner_with_theme(
    frame: &mut Frame,
    inner: Rect,
    content: Option<&PreviewContent>,
    theme: &Theme,
) {
    match content {
        Some(PreviewContent::Text(text_preview)) => {
            render_text_preview(frame, inner, text_preview, theme);
        }
        Some(PreviewContent::Image(image_preview)) => {
            render_image_preview(frame, inner, image_preview, theme);
        }
        Some(PreviewContent::Pdf(pdf_preview)) => {
            render_pdf_preview(frame, inner, pdf_preview, theme);
        }
        Some(PreviewContent::Archive(archive_preview)) => {
            render_archive_preview(frame, inner, archive_preview, theme);
        }
        Some(PreviewContent::Directory(dir_preview)) => {
            render_directory_preview(frame, inner, dir_preview, theme);
        }
        Some(PreviewContent::Metadata(metadata_preview)) => {
            render_metadata_preview(frame, inner, metadata_preview, theme);
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
            let placeholder = Paragraph::new("Select a file to preview it.")
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
fn render_metadata_preview(frame: &mut Frame, area: Rect, meta: &MetadataPreview, theme: &Theme) {
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
fn render_text_preview(
    frame: &mut Frame,
    area: Rect,
    preview: &crate::preview::TextPreview,
    theme: &Theme,
) {
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

/// Renders structured image preview metadata and formatted dimensions card.
fn render_image_preview(frame: &mut Frame, area: Rect, img: &ImagePreview, theme: &Theme) {
    let total_width = area.width as usize;
    let max_rows = area.height as usize;

    let mut lines = Vec::new();

    // Header badge
    let header_badge = format!(" [IMAGE: {}] ", img.format.short_name());
    lines.push(Line::from(vec![
        Span::styled(
            header_badge,
            theme
                .tab_active_focused
                .bg(ratatui::style::Color::Magenta)
                .fg(ratatui::style::Color::White),
        ),
        Span::raw(" "),
        Span::styled(
            format!("[Graphics: {}]", img.graphics_protocol.display_name()),
            theme.preview_muted,
        ),
    ]));
    lines.push(Line::default());

    // Details
    let size_str = crate::preview::format_size(img.file_size);
    let dims_str = if img.width > 0 && img.height > 0 {
        format!("{} × {} px", img.width, img.height)
    } else {
        "Unknown (header unread)".into()
    };
    let ratio_str = img.aspect_ratio_str();

    let fields = [
        (
            "Format",
            img.format.display_name(),
            theme.preview_metadata_value,
        ),
        ("Dimensions", &dims_str, theme.header_path),
        ("Aspect Ratio", &ratio_str, theme.preview_metadata_label),
        ("Color Model", &img.color_info, theme.preview_metadata_value),
        ("File Size", &size_str, theme.header_path),
        (
            "Terminal Protocol",
            img.graphics_protocol.display_name(),
            theme.preview_line_number,
        ),
    ];

    for (label, val, val_style) in fields {
        if lines.len() >= max_rows {
            break;
        }
        let label_fmt = format!("{:<16} ", format!("{label}:"));
        let line = Line::from(vec![
            Span::styled(label_fmt, theme.preview_metadata_label),
            Span::styled(val.to_string(), val_style),
        ]);
        lines.push(line);
    }

    // Visual aspect ratio framing if vertical room permits
    if max_rows > lines.len() + 4 && total_width > 20 && img.width > 0 && img.height > 0 {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "┌ Visual Canvas Framing ─────────────────┐",
            theme.preview_border,
        )));
        let frame_w = total_width.min(36).saturating_sub(4);
        let frame_h = (max_rows.saturating_sub(lines.len() + 2)).min(6);
        for row in 0..frame_h {
            let row_str = if row == frame_h / 2 {
                let mid_label = format!("  {}  ", dims_str);
                let pad = frame_w.saturating_sub(mid_label.len()) / 2;
                format!("│{:pad$}{mid_label}{:pad$}│", "", "", pad = pad)
            } else {
                format!("│{:width$}│", "", width = frame_w)
            };
            lines.push(Line::from(Span::styled(row_str, theme.preview_muted)));
        }
        lines.push(Line::from(Span::styled(
            "└────────────────────────────────────────┘",
            theme.preview_border,
        )));
    }

    let paragraph = Paragraph::new(lines.into_iter().take(max_rows).collect::<Vec<_>>());
    frame.render_widget(paragraph, area);
}

/// Renders structured PDF document summary preview.
fn render_pdf_preview(frame: &mut Frame, area: Rect, pdf: &PdfPreview, theme: &Theme) {
    let max_rows = area.height as usize;
    let mut lines = Vec::new();

    // Header badge
    lines.push(Line::from(vec![
        Span::styled(
            " [PDF DOCUMENT] ",
            theme
                .tab_active_focused
                .bg(ratatui::style::Color::Red)
                .fg(ratatui::style::Color::White),
        ),
        Span::raw(" "),
        Span::styled(&pdf.version, theme.header_path),
    ]));
    lines.push(Line::default());

    let page_str = pdf
        .page_count
        .map(|c| format!("{c} pages"))
        .unwrap_or_else(|| "Unknown".to_string());
    let size_str = crate::preview::format_size(pdf.file_size);

    let mut fields: Vec<(&str, String)> = vec![
        ("Version", pdf.version.clone()),
        ("Page Count", page_str),
        ("File Size", size_str),
    ];

    if let Some(t) = &pdf.title {
        fields.push(("Title", t.clone()));
    }
    if let Some(a) = &pdf.author {
        fields.push(("Author", a.clone()));
    }
    if let Some(c) = &pdf.creator {
        fields.push(("Creator", c.clone()));
    }
    if let Some(p) = &pdf.producer {
        fields.push(("Producer", p.clone()));
    }

    for (label, val) in fields {
        if lines.len() >= max_rows {
            break;
        }
        let label_fmt = format!("{:<14} ", format!("{label}:"));
        lines.push(Line::from(vec![
            Span::styled(label_fmt, theme.preview_metadata_label),
            Span::styled(val, theme.preview_metadata_value),
        ]));
    }

    if !pdf.sample_text.is_empty() && max_rows > lines.len() + 2 {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "Extracted Text Snippet:",
            theme.header_path,
        )));
        for snippet in &pdf.sample_text {
            if lines.len() >= max_rows {
                break;
            }
            lines.push(Line::from(Span::styled(
                format!("  • {snippet}"),
                theme.preview_line_number,
            )));
        }
    }

    let paragraph = Paragraph::new(lines.into_iter().take(max_rows).collect::<Vec<_>>());
    frame.render_widget(paragraph, area);
}

/// Renders archive table and contents listing preview.
fn render_archive_preview(frame: &mut Frame, area: Rect, arc: &ArchivePreview, theme: &Theme) {
    let max_rows = area.height as usize;
    let total_width = area.width as usize;
    let mut lines = Vec::new();

    // Header badge
    let uncomp_str = arc
        .total_uncompressed_size
        .map(crate::preview::format_size)
        .unwrap_or_else(|| "--".to_string());
    lines.push(Line::from(vec![
        Span::styled(
            format!(" [{}] ", arc.format),
            theme
                .tab_active_focused
                .bg(ratatui::style::Color::Yellow)
                .fg(ratatui::style::Color::Black),
        ),
        Span::raw(" "),
        Span::styled(
            format!(
                "{} entries • Compressed: {} (Uncompressed: {})",
                arc.total_entries,
                crate::preview::format_size(arc.file_size),
                uncomp_str
            ),
            theme.preview_metadata_value,
        ),
    ]));
    lines.push(Line::default());

    if arc.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "Archive contents (preview only — not extracted)",
            theme.preview_muted,
        )));
    } else {
        lines.push(Line::from(vec![Span::styled(
            "  #  │ Name / Path",
            theme.preview_metadata_label,
        )]));
        lines.push(Line::from(Span::styled(
            "─────┼────────────────────────────────────────────",
            theme.preview_border,
        )));

        for (idx, entry) in arc.entries.iter().enumerate() {
            if lines.len() >= max_rows {
                break;
            }
            let idx_str = format!("{:>3} │ ", idx + 1);
            let icon = if entry.is_directory { "📁 " } else { "📄 " };
            let size_tag = entry
                .size
                .map(|s| format!(" ({})", crate::preview::format_size(s)))
                .unwrap_or_default();

            let full_name = format!("{icon}{}{size_tag}", entry.path);
            let avail = total_width.saturating_sub(display_width(&idx_str));
            let name_trunc = truncate_to_width(&full_name, avail);

            lines.push(Line::from(vec![
                Span::styled(idx_str, theme.preview_line_number),
                Span::styled(
                    name_trunc,
                    if entry.is_directory {
                        theme.entry_directory
                    } else {
                        theme.preview_metadata_value
                    },
                ),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines.into_iter().take(max_rows).collect::<Vec<_>>());
    frame.render_widget(paragraph, area);
}

/// Renders directory summary and statistics preview.
fn render_directory_preview(frame: &mut Frame, area: Rect, dir: &DirectoryPreview, theme: &Theme) {
    let max_rows = area.height as usize;
    let mut lines = Vec::new();

    // Header badge
    lines.push(Line::from(vec![
        Span::styled(
            " [DIRECTORY] ",
            theme
                .tab_active_focused
                .bg(ratatui::style::Color::Blue)
                .fg(ratatui::style::Color::White),
        ),
        Span::raw(" "),
        Span::styled(&dir.name, theme.header_path.add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::default());

    let fields = [
        ("Path", dir.path.display().to_string()),
        ("Total Items", dir.item_count.to_string()),
        ("Files", dir.file_count.to_string()),
        ("Subdirectories", dir.dir_count.to_string()),
        ("Symlinks", dir.symlink_count.to_string()),
        (
            "Immediate Size",
            crate::preview::format_size(dir.immediate_size),
        ),
        (
            "Git Context",
            dir.git_status.clone().unwrap_or_else(|| "None".to_string()),
        ),
    ];

    for (label, val) in fields {
        if lines.len() >= max_rows {
            break;
        }
        let label_fmt = format!("{:<16} ", format!("{label}:"));
        lines.push(Line::from(vec![
            Span::styled(label_fmt, theme.preview_metadata_label),
            Span::styled(val, theme.preview_metadata_value),
        ]));
    }

    if !dir.sample_entries.is_empty() && max_rows > lines.len() + 2 {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "Contained Entries:",
            theme.header_path,
        )));
        for entry_name in &dir.sample_entries {
            if lines.len() >= max_rows {
                break;
            }
            lines.push(Line::from(Span::styled(
                format!("  {entry_name}"),
                theme.preview_metadata_value,
            )));
        }
    }

    let paragraph = Paragraph::new(lines.into_iter().take(max_rows).collect::<Vec<_>>());
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
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

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
}
