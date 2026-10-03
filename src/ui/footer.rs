//! The application footer and status interface.
//!
//! Renders a compact, responsive status bar displaying current mode, active pane,
//! selection information, entry count, clipboard state (when non-empty), active keyboard
//! shortcuts, or notifications/errors when present.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::actions::Action;
use crate::app::modes::Mode;
use crate::app::state::{ActivePane, App, ClipboardOperation, ClipboardState, NotificationState};
use crate::ui::{display_width, truncate_to_width};

/// Formats the display name for an application [`Mode`].
pub fn mode_display(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "NORMAL",
        Mode::Search => "SEARCH",
        Mode::Rename => "RENAME",
        Mode::Create => "CREATE",
        Mode::Confirm => "CONFIRM",
        Mode::Preview => "PREVIEW",
        Mode::CommandPalette => "COMMAND PALETTE",
        Mode::Help => "HELP",
        Mode::Bookmarks => "BOOKMARKS",
        Mode::Jump => "JUMP TO",
        Mode::SmartJump => "SMART JUMP",
        Mode::ProjectCockpit => "PROJECT",
        Mode::GitStatusPanel => "GIT STATUS",
        Mode::FileRadar => "FILE RADAR",
        Mode::RevealContext => "CONTEXT",
        Mode::ContextMenu => "MENU",
        Mode::Terminal => "TERMINAL",
        Mode::StorageVision => "STORAGE VISION",
        Mode::ThemeSelector => "THEME SELECTOR",
    }
}

/// Formats the display name for the [`ActivePane`].
pub fn pane_display(pane: ActivePane) -> &'static str {
    match pane {
        ActivePane::Left => "LEFT",
        ActivePane::Right => "RIGHT",
    }
}

/// Formats selection and entry count information in human-friendly 1-based indexing.
///
/// For a non-empty listing with a selected item, returns `"X / Y"` (e.g., `"4 / 27"`).
/// For an empty listing, returns `"0 / 0"`.
/// If no item is selected in a non-empty listing, returns `"0 / Y"`.
pub fn selection_display(selected: Option<usize>, total: usize) -> String {
    if total == 0 {
        return "0 / 0".to_string();
    }
    match selected {
        Some(index) => {
            let human_index = (index + 1).min(total);
            format!("{human_index} / {total}")
        }
        None => format!("0 / {total}"),
    }
}

/// Formats the active clipboard operation indicator.
///
/// Returns `Some("COPY")` or `Some("CUT")` when the clipboard has entries,
/// or `None` when the clipboard is empty.
pub fn clipboard_display(clipboard: &ClipboardState) -> Option<&'static str> {
    if clipboard.is_empty() {
        return None;
    }
    match clipboard.operation() {
        Some(ClipboardOperation::Copy) => Some("COPY"),
        Some(ClipboardOperation::Cut) => Some("CUT"),
        None => None,
    }
}

/// Formats the notification message text if active.
pub fn notification_display(notification: &NotificationState) -> Option<&str> {
    if notification.is_active() {
        notification.message()
    } else {
        None
    }
}

/// Renders the compact footer / status bar inside `area`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let theme = app.theme();
    let width = area.width as usize;

    // Error / Notification display (Priority 1)
    if app.notification().is_active() {
        let msg = app.notification().message().unwrap_or("Error");
        let prefix = format!(" {} ", theme.symbols.error_prefix);
        let prefix_width = display_width(&prefix);

        if width <= prefix_width {
            let truncated_prefix = truncate_to_width(&prefix, width);
            let span = Span::styled(truncated_prefix, theme.notify_error_badge);
            frame.render_widget(Paragraph::new(Line::from(vec![span])), area);
            return;
        }

        let available = width.saturating_sub(prefix_width + 1);
        let truncated_msg = truncate_to_width(msg, available);

        let line = Line::from(vec![
            Span::styled(prefix, theme.notify_error_badge),
            Span::styled(format!(" {truncated_msg}"), theme.notify_error_text),
        ]);

        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
        return;
    }

    // Normal Status Bar: Assemble responsive segments according to priority.
    let mode_str = mode_display(app.mode());
    let pane_str = pane_display(app.active_pane());
    let active_pane = app.pane(app.active_pane());
    let sel_count = active_pane.selected_count();
    let base_sel = selection_display(active_pane.selected_index(), active_pane.visible_count());
    let sel_str = if sel_count > 0 {
        let size = active_pane.aggregate_selected_size();
        if size > 0 && width >= 80 {
            format!(
                "[{sel_count} sel • {}] {base_sel}",
                crate::preview::format_size(size)
            )
        } else {
            format!("[{sel_count} sel] {base_sel}")
        }
    } else {
        base_sel
    };
    let clip_op = clipboard_display(app.clipboard());
    let hint_str = "Ctrl+C: Quit";

    // Extreme narrow width fallback: truncate mode string.
    let mode_text = format!(" {mode_str} ");
    let mode_text_width = display_width(&mode_text);
    if width < mode_text_width {
        let truncated_mode = truncate_to_width(mode_str, width);
        let span = Span::styled(truncated_mode, theme.footer_mode);
        frame.render_widget(Paragraph::new(Line::from(vec![span])), area);
        return;
    }

    let sep = Span::styled(
        format!("{} ", theme.symbols.vertical_separator),
        theme.footer_separator,
    );
    let sep_width = display_width(&format!("{} ", theme.symbols.vertical_separator));

    // Search mode: render dedicated search prompt and live query.
    if app.mode() == crate::app::modes::Mode::Search {
        let query = app.search().query();
        let search_mode_label = match app.search().mode() {
            crate::search::SearchMode::Basic => "BASIC",
            crate::search::SearchMode::Recursive => "RECURSIVE",
            crate::search::SearchMode::Fuzzy => "FUZZY",
            crate::search::SearchMode::RecursiveFuzzy => "REC.FUZZY",
        };
        let match_count = app.pane(app.active_pane()).visible_count();
        let search_text = format!(" Search: {query}█ ");
        let sm_text = format!("[{search_mode_label}] ({match_count} matches) ");
        let hint_search = "Enter: Done • Esc: Cancel • Tab: Mode";

        let mut spans = vec![
            Span::styled(" SEARCH ", theme.footer_mode),
            sep.clone(),
            Span::styled(search_text, theme.palette_selected),
            Span::styled(sm_text, theme.footer_hint),
        ];
        let cur_w: usize = spans.iter().map(|s| display_width(&s.content)).sum();
        let hint_w = display_width(hint_search);
        if width > cur_w + hint_w {
            let padding = width - cur_w - hint_w;
            spans.push(Span::raw(" ".repeat(padding)));
            spans.push(Span::styled(hint_search, theme.footer_hint));
        }
        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    let mut left_spans = Vec::new();

    // 1. Mode (Priority 2)
    left_spans.push(Span::styled(mode_text, theme.footer_mode));
    let mut current_width = mode_text_width;

    // 1b. Focus Mode indicator
    if app.is_focus_mode() {
        let focus_text = "[FOCUS] ";
        let focus_width = display_width(focus_text);
        if current_width + sep_width + focus_width <= width {
            left_spans.push(sep.clone());
            left_spans.push(Span::styled(focus_text, theme.palette_shortcut));
            current_width += sep_width + focus_width;
        }
    }

    // 1c. Search mode indicator (shown when in Search mode).
    if app.mode() == crate::app::modes::Mode::Search {
        use crate::search::SearchMode;
        let search_mode_label = match app.search().mode() {
            SearchMode::Basic => "BASIC",
            SearchMode::Recursive => "RECURSIVE",
            SearchMode::Fuzzy => "FUZZY",
            SearchMode::RecursiveFuzzy => "REC.FUZZY",
        };
        let sm_text = format!("{search_mode_label} ");
        let sm_width = display_width(&sm_text);
        if current_width + sep_width + sm_width <= width {
            left_spans.push(sep.clone());
            left_spans.push(Span::styled(sm_text, theme.footer_hint));
            current_width += sep_width + sm_width;
        }
    }

    // 2. Active Pane (Priority 3)
    let pane_text = format!("{pane_str} ");
    let pane_width = display_width(&pane_text);
    if current_width + sep_width + pane_width <= width {
        left_spans.push(sep.clone());
        left_spans.push(Span::styled(pane_text, theme.footer_pane));
        current_width += sep_width + pane_width;

        // 3. Selection & Entry Count (Priority 4)
        let sel_text = format!("{sel_str} ");
        let sel_width = display_width(&sel_text);
        if current_width + sep_width + sel_width <= width {
            left_spans.push(sep.clone());
            left_spans.push(Span::styled(sel_text, theme.footer_selection));
            current_width += sep_width + sel_width;

            // 4. Clipboard (Priority 5)
            if let Some(clip) = clip_op {
                let clip_text = format!("{clip} ");
                let clip_width = display_width(&clip_text);
                if current_width + sep_width + clip_width <= width {
                    left_spans.push(sep.clone());
                    left_spans.push(Span::styled(clip_text, theme.footer_clipboard));
                    current_width += sep_width + clip_width;
                }
            }

            // 4b. Navigation history arrows (◀ back  ▶ forward)
            let can_back = active_pane.can_go_back();
            let can_fwd = active_pane.can_go_forward();
            if can_back || can_fwd {
                let back = if can_back { "◀ " } else { "  " };
                let fwd = if can_fwd { "▶" } else { " " };
                let nav_text = format!("{back}{fwd} ");
                let nav_width = display_width(&nav_text);
                if current_width + sep_width + nav_width <= width {
                    left_spans.push(sep.clone());
                    left_spans.push(Span::styled(nav_text, theme.footer_hint));
                    current_width += sep_width + nav_width;
                }
            }
        }
    }

    // 5. Adaptive Actions / Keyboard Hints (Priority 6)
    if app.mode() == Mode::Normal {
        let actions = adaptive_actions(app, width);
        let mut action_spans = Vec::new();
        let mut actions_width = 0;

        for (i, action) in actions.iter().enumerate() {
            if i > 0 {
                action_spans.push(Span::raw(" "));
                actions_width += 1;
            }
            let btn_text = format!("[{}]", action.label);
            actions_width += display_width(&btn_text);
            action_spans.push(Span::styled(btn_text, theme.palette_shortcut));
        }

        let hint_width = display_width(hint_str);
        if width > current_width + hint_width + actions_width + 4 {
            let padding = width - current_width - hint_width - actions_width - 2;
            left_spans.push(Span::raw(" ".repeat(padding)));
            left_spans.push(Span::styled(hint_str, theme.footer_hint));
            left_spans.push(Span::raw("  "));
            left_spans.extend(action_spans);
        } else if width > current_width + hint_width {
            let padding = width - current_width - hint_width;
            left_spans.push(Span::raw(" ".repeat(padding)));
            left_spans.push(Span::styled(hint_str, theme.footer_hint));
        } else if width > current_width + actions_width + 2 {
            let padding = width - current_width - actions_width - 1;
            left_spans.push(Span::raw(" ".repeat(padding)));
            left_spans.extend(action_spans);
        }
    } else {
        let hint_width = display_width(hint_str);
        if width > current_width + hint_width {
            let padding = width - current_width - hint_width;
            left_spans.push(Span::raw(" ".repeat(padding)));
            left_spans.push(Span::styled(hint_str, theme.footer_hint));
        }
    }

    let line = Line::from(left_spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

/// An adaptive action exposed in the footer action area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveAction {
    pub action: Action,
    pub label: &'static str,
}

/// Generates the most relevant contextual actions based on selection and available width.
pub fn adaptive_actions(app: &App, available_width: usize) -> Vec<AdaptiveAction> {
    let active_pane = app.pane(app.active_pane());
    let paths = active_pane.effective_selected_paths();

    if paths.len() > 1 {
        // Multiple selected
        if available_width >= 90 {
            vec![
                AdaptiveAction {
                    action: Action::Copy,
                    label: "Copy",
                },
                AdaptiveAction {
                    action: Action::Cut,
                    label: "Cut",
                },
                AdaptiveAction {
                    action: Action::Delete,
                    label: "Delete",
                },
                AdaptiveAction {
                    action: Action::GetInfo,
                    label: "Info",
                },
                AdaptiveAction {
                    action: Action::ContextMenu,
                    label: "More",
                },
            ]
        } else if available_width >= 60 {
            vec![
                AdaptiveAction {
                    action: Action::Copy,
                    label: "Copy",
                },
                AdaptiveAction {
                    action: Action::Delete,
                    label: "Delete",
                },
                AdaptiveAction {
                    action: Action::ContextMenu,
                    label: "More",
                },
            ]
        } else {
            vec![
                AdaptiveAction {
                    action: Action::Delete,
                    label: "Delete",
                },
                AdaptiveAction {
                    action: Action::ContextMenu,
                    label: "More",
                },
            ]
        }
    } else if let Some(path) = paths.first() {
        let is_dir = active_pane
            .selected_entry()
            .map(crate::filesystem::entry::Entry::is_dir)
            .unwrap_or_else(|| path.is_dir());
        if is_dir {
            // Folder selected
            if available_width >= 100 {
                vec![
                    AdaptiveAction {
                        action: Action::Open,
                        label: "Open",
                    },
                    AdaptiveAction {
                        action: Action::Copy,
                        label: "Copy",
                    },
                    AdaptiveAction {
                        action: Action::Cut,
                        label: "Cut",
                    },
                    AdaptiveAction {
                        action: Action::Rename,
                        label: "Rename",
                    },
                    AdaptiveAction {
                        action: Action::Delete,
                        label: "Delete",
                    },
                    AdaptiveAction {
                        action: Action::ContextMenu,
                        label: "More",
                    },
                ]
            } else if available_width >= 70 {
                vec![
                    AdaptiveAction {
                        action: Action::Open,
                        label: "Open",
                    },
                    AdaptiveAction {
                        action: Action::Copy,
                        label: "Copy",
                    },
                    AdaptiveAction {
                        action: Action::Rename,
                        label: "Rename",
                    },
                    AdaptiveAction {
                        action: Action::ContextMenu,
                        label: "More",
                    },
                ]
            } else {
                vec![
                    AdaptiveAction {
                        action: Action::Open,
                        label: "Open",
                    },
                    AdaptiveAction {
                        action: Action::ContextMenu,
                        label: "More",
                    },
                ]
            }
        } else {
            // File selected
            if available_width >= 100 {
                vec![
                    AdaptiveAction {
                        action: Action::Open,
                        label: "Open",
                    },
                    AdaptiveAction {
                        action: Action::Preview,
                        label: "Preview",
                    },
                    AdaptiveAction {
                        action: Action::Copy,
                        label: "Copy",
                    },
                    AdaptiveAction {
                        action: Action::Rename,
                        label: "Rename",
                    },
                    AdaptiveAction {
                        action: Action::Delete,
                        label: "Delete",
                    },
                    AdaptiveAction {
                        action: Action::ContextMenu,
                        label: "More",
                    },
                ]
            } else if available_width >= 70 {
                vec![
                    AdaptiveAction {
                        action: Action::Open,
                        label: "Open",
                    },
                    AdaptiveAction {
                        action: Action::Copy,
                        label: "Copy",
                    },
                    AdaptiveAction {
                        action: Action::Rename,
                        label: "Rename",
                    },
                    AdaptiveAction {
                        action: Action::ContextMenu,
                        label: "More",
                    },
                ]
            } else {
                vec![
                    AdaptiveAction {
                        action: Action::Open,
                        label: "Open",
                    },
                    AdaptiveAction {
                        action: Action::ContextMenu,
                        label: "More",
                    },
                ]
            }
        }
    } else {
        // Nothing selected / empty directory
        if available_width >= 90 {
            vec![
                AdaptiveAction {
                    action: Action::NewDirectory,
                    label: "New Folder",
                },
                AdaptiveAction {
                    action: Action::NewFile,
                    label: "New File",
                },
                AdaptiveAction {
                    action: Action::StartSearch,
                    label: "Search",
                },
                AdaptiveAction {
                    action: Action::RefreshDirectory,
                    label: "Refresh",
                },
                AdaptiveAction {
                    action: Action::ContextMenu,
                    label: "More",
                },
            ]
        } else if available_width >= 60 {
            vec![
                AdaptiveAction {
                    action: Action::NewDirectory,
                    label: "New Folder",
                },
                AdaptiveAction {
                    action: Action::NewFile,
                    label: "New File",
                },
                AdaptiveAction {
                    action: Action::ContextMenu,
                    label: "More",
                },
            ]
        } else {
            vec![
                AdaptiveAction {
                    action: Action::NewDirectory,
                    label: "New",
                },
                AdaptiveAction {
                    action: Action::ContextMenu,
                    label: "More",
                },
            ]
        }
    }
}

/// Calculates hit testing ranges for adaptive action buttons in the footer.
/// Returns `(action, start_x, end_x, y)`.
pub fn action_hit_ranges(area: Rect, app: &App) -> Vec<(Action, u16, u16, u16)> {
    if area.height == 0 || area.width == 0 || app.mode() != Mode::Normal {
        return Vec::new();
    }

    let actions = adaptive_actions(app, area.width as usize);
    let mut total_actions_w = 0;
    for (i, a) in actions.iter().enumerate() {
        total_actions_w += display_width(a.label) + 2; // [Label]
        if i + 1 < actions.len() {
            total_actions_w += 1; // space
        }
    }

    if (area.width as usize) < total_actions_w + 35 {
        return Vec::new();
    }

    let start_x = (area.x + area.width).saturating_sub(total_actions_w as u16 + 1);
    let mut current_x = start_x;
    let mut ranges = Vec::new();

    for a in actions {
        let btn_w = (display_width(a.label) + 2) as u16;
        ranges.push((a.action, current_x, current_x + btn_w, area.y));
        current_x += btn_w + 1;
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::Action;
    use crate::filesystem::test_support::TempDir;
    use crate::search::SearchMode;
    use crate::ui::theme::Theme;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::fs;

    #[test]
    fn test_1_normal_mode_footer() {
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
        assert!(content.contains("NORMAL"), "footer must show NORMAL mode");
        assert!(
            content.contains("Ctrl+C: Quit"),
            "footer must show quit shortcut"
        );
    }

    #[test]
    fn test_2_search_mode_footer() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        app.handle_action(Action::StartSearch);

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..80).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(content.contains("SEARCH"), "footer must show SEARCH mode");
    }

    #[test]
    fn test_3_active_left_pane() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();
        assert_eq!(app.active_pane(), ActivePane::Left);

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..80).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(content.contains("LEFT"), "footer must show LEFT pane");
    }

    #[test]
    fn test_4_active_right_pane() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::default();
        app.handle_action(Action::SwitchPane);
        assert_eq!(app.active_pane(), ActivePane::Right);

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..80).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(content.contains("RIGHT"), "footer must show RIGHT pane");
    }

    #[test]
    fn test_5_empty_pane_selection() {
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
        assert!(
            content.contains("0 / 0"),
            "empty pane must show 0 / 0 selection"
        );
    }

    #[test]
    fn test_6_non_empty_pane_selection() {
        let temp = TempDir::new("footer-non-empty");
        fs::write(temp.path().join("a.txt"), "hello").unwrap();
        fs::write(temp.path().join("b.txt"), "world").unwrap();
        fs::write(temp.path().join("c.txt"), "!").unwrap();

        let mut app = App::at(temp.path().to_path_buf()).unwrap();
        // Initially first item (0-based 0) is selected -> human 1 / 3
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..80).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(
            content.contains("1 / 3"),
            "first of three entries must show 1 / 3"
        );

        // Move down -> 2 / 3
        app.handle_action(Action::MoveDown);
        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..80).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(
            content.contains("2 / 3"),
            "second of three entries must show 2 / 3"
        );
    }

    #[test]
    fn test_7_selection_numbering_helper() {
        assert_eq!(selection_display(None, 0), "0 / 0");
        assert_eq!(selection_display(None, 10), "0 / 10");
        assert_eq!(selection_display(Some(0), 10), "1 / 10");
        assert_eq!(selection_display(Some(3), 27), "4 / 27");
        assert_eq!(selection_display(Some(26), 27), "27 / 27");
        // Out of range safely clamped
        assert_eq!(selection_display(Some(50), 27), "27 / 27");
    }

    #[test]
    fn test_8_clipboard_copy() {
        let temp = TempDir::new("footer-clip-copy");
        fs::write(temp.path().join("a.txt"), "hello").unwrap();

        let mut app = App::at(temp.path().to_path_buf()).unwrap();
        app.mark_for_copy();

        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..80).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(
            content.contains("COPY"),
            "footer must show COPY when clipboard has copy"
        );
    }

    #[test]
    fn test_9_clipboard_cut() {
        let temp = TempDir::new("footer-clip-cut");
        fs::write(temp.path().join("a.txt"), "hello").unwrap();

        let mut app = App::at(temp.path().to_path_buf()).unwrap();
        app.mark_for_cut();

        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..80).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(
            content.contains("CUT"),
            "footer must show CUT when clipboard has cut"
        );
    }

    #[test]
    fn test_10_empty_clipboard() {
        let app = App::default();
        assert_eq!(clipboard_display(app.clipboard()), None);

        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..80).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(
            !content.contains("COPY"),
            "empty clipboard must not show COPY"
        );
        assert!(
            !content.contains("CUT"),
            "empty clipboard must not show CUT"
        );
    }

    #[test]
    fn test_11_error_notification_display() {
        let mut app = App::default();
        app.set_search_mode(SearchMode::Recursive);
        // Force search on invalid path to trigger notification
        app.run_search("something");
        assert!(app.notification().is_active());

        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..80).map(|x| buffer[(x, 0)].symbol()).collect();
        let theme = Theme::default();
        assert!(
            content.contains(theme.symbols.error_prefix) || content.contains("[!]"),
            "footer must show error prefix"
        );
    }

    #[test]
    fn test_12_long_notification_truncation() {
        let mut app = App::default();
        app.set_search_mode(SearchMode::Recursive);
        app.run_search("needle");

        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..40).map(|x| buffer[(x, 0)].symbol()).collect();
        let theme = Theme::default();
        assert_eq!(content.chars().count(), 40);
        assert!(content.contains(theme.symbols.error_prefix) || content.contains("[!]"));
    }

    #[test]
    fn test_13_unicode_notification() {
        let mut app = App::default();
        app.set_search_mode(SearchMode::Recursive);
        app.run_search("🦀_test");

        let backend = TestBackend::new(50, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..50).map(|x| buffer[(x, 0)].symbol()).collect();
        let theme = Theme::default();
        assert!(content.contains(theme.symbols.error_prefix) || content.contains("[!]"));
    }

    #[test]
    fn test_14_narrow_footer() {
        for width in [20, 25, 30, 40] {
            let backend = TestBackend::new(width, 1);
            let mut terminal = Terminal::new(backend).unwrap();
            let app = App::default();

            terminal
                .draw(|f| {
                    render(f, f.area(), &app);
                })
                .unwrap();

            let buffer = terminal.backend().buffer();
            let content: String = (0..width).map(|x| buffer[(x, 0)].symbol()).collect();
            assert_eq!(content.chars().count(), width as usize);
            assert!(
                content.contains("NORMAL"),
                "narrow footer must prioritize mode"
            );
        }
    }

    #[test]
    fn test_15_wide_footer() {
        for width in [120, 160, 200] {
            let backend = TestBackend::new(width, 1);
            let mut terminal = Terminal::new(backend).unwrap();
            let app = App::default();

            terminal
                .draw(|f| {
                    render(f, f.area(), &app);
                })
                .unwrap();

            let buffer = terminal.backend().buffer();
            let content: String = (0..width).map(|x| buffer[(x, 0)].symbol()).collect();
            assert!(content.contains("NORMAL"));
            assert!(content.contains("LEFT"));
            assert!(content.contains("0 / 0"));
            assert!(content.contains("Ctrl+C: Quit"));
        }
    }

    #[test]
    fn test_16_very_small_terminal() {
        for width in [0, 1, 2, 5, 8, 10] {
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
    fn test_17_no_text_overflow() {
        let temp = TempDir::new("footer-overflow");
        for i in 0..100 {
            fs::write(temp.path().join(format!("file_{i}.txt")), "test").unwrap();
        }
        let app = App::at(temp.path().to_path_buf()).unwrap();

        for width in [15, 22, 35, 47, 80, 100, 159, 160, 200] {
            let backend = TestBackend::new(width, 1);
            let mut terminal = Terminal::new(backend).unwrap();

            terminal
                .draw(|f| {
                    render(f, f.area(), &app);
                })
                .unwrap();

            let buffer = terminal.backend().buffer();
            let content: String = (0..width).map(|x| buffer[(x, 0)].symbol()).collect();
            assert_eq!(content.chars().count(), width as usize);
        }
    }

    #[test]
    fn test_18_no_invalid_selection_display() {
        assert_eq!(selection_display(None, 0), "0 / 0");
        assert_eq!(selection_display(Some(100), 5), "5 / 5");
        assert_eq!(selection_display(Some(0), 1), "1 / 1");
    }

    #[test]
    fn test_19_rendering_does_not_modify_app_state() {
        let temp = TempDir::new("footer-immutability");
        fs::write(temp.path().join("a.txt"), "hello").unwrap();
        let app = App::at(temp.path().to_path_buf()).unwrap();
        let before = app.clone();

        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                render(f, f.area(), &app);
            })
            .unwrap();

        assert_eq!(app, before, "rendering must not mutate application state");
    }

    #[test]
    fn test_mode_display_all_variants() {
        assert_eq!(mode_display(Mode::Normal), "NORMAL");
        assert_eq!(mode_display(Mode::Search), "SEARCH");
        assert_eq!(mode_display(Mode::Rename), "RENAME");
        assert_eq!(mode_display(Mode::Create), "CREATE");
        assert_eq!(mode_display(Mode::Confirm), "CONFIRM");
        assert_eq!(mode_display(Mode::Preview), "PREVIEW");
        assert_eq!(mode_display(Mode::CommandPalette), "COMMAND PALETTE");
        assert_eq!(mode_display(Mode::Help), "HELP");
        assert_eq!(mode_display(Mode::Bookmarks), "BOOKMARKS");
        assert_eq!(mode_display(Mode::Jump), "JUMP TO");
        assert_eq!(mode_display(Mode::SmartJump), "SMART JUMP");
        assert_eq!(mode_display(Mode::ThemeSelector), "THEME SELECTOR");
    }

    #[test]
    fn test_pane_display_variants() {
        assert_eq!(pane_display(ActivePane::Left), "LEFT");
        assert_eq!(pane_display(ActivePane::Right), "RIGHT");
    }
}
