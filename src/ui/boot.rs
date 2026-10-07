//! Vision Boot signature startup rendering (Phase 2.2).
//!
//! Renders the progressive startup sequence:
//! Wake -> Identity -> System Readiness -> Project Awareness -> Interface Construction & Vision Pulse.
//!
//! Features terminal capability degradation, small screen safety, Unicode/ASCII fallback,
//! and complete adherence to semantic theme tokens without hardcoded colors.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::animation::boot::{BootPhase, BootState};
use crate::app::state::App;
use crate::ui::display_width;
use crate::ui::theme::{Symbols, Theme};

/// Renders the complete Vision Boot sequence according to current progress.
pub fn render(frame: &mut Frame, area: Rect, app: &App, boot_state: &BootState, progress: f32) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let theme = app.theme();
    let symbols = theme.symbols;
    let phase = BootPhase::from_progress(progress, boot_state.startup_mode);
    let local_p = phase.local_progress(progress, boot_state.startup_mode);

    // If in the final Vision Pulse / Interface Construction phase, render the live UI underneath
    if phase == BootPhase::VisionPulse {
        render_vision_pulse_phase(frame, area, app, boot_state, local_p, &theme, &symbols);
        return;
    }

    // Handle very small terminals (< 60 columns or < 14 rows)
    if area.width < 60 || area.height < 14 {
        render_compact_fallback(frame, area, app, boot_state, phase, &theme, &symbols);
        return;
    }

    // Standard centered layout canvas
    frame.render_widget(Clear, area);

    // Background fill with theme background
    let bg_block = Block::default().style(Style::default().bg(theme.palette.background));
    frame.render_widget(bg_block, area);

    match phase {
        BootPhase::Wake => render_wake(frame, area, local_p, &theme, &symbols),
        BootPhase::Identity => render_identity(frame, area, local_p, &theme, &symbols),
        BootPhase::SystemReadiness => {
            render_system_readiness(frame, area, boot_state, local_p, &theme, &symbols)
        }
        BootPhase::ProjectAwareness => {
            render_project_awareness(frame, area, boot_state, local_p, &theme, &symbols)
        }
        BootPhase::VisionPulse | BootPhase::Ready => {}
    }
}

/// Renders Phase 1: Minimal central visual signal / expanding pulse.
fn render_wake(frame: &mut Frame, area: Rect, local_p: f32, theme: &Theme, symbols: &Symbols) {
    let center_y = area.y + area.height / 2;
    let dot = symbols.focus_bullet;
    let line_char = if symbols.focus_bullet == "*" {
        "-"
    } else {
        "─"
    };

    let line_content = if local_p < 0.35 {
        Line::from(vec![Span::styled(
            dot,
            theme.primary().add_modifier(Modifier::BOLD),
        )])
    } else {
        // Expand horizontal wings from 2 to 14 chars
        let wing_len = ((local_p - 0.35) / 0.65 * 12.0) as usize + 2;
        let wings_left = line_char.repeat(wing_len);
        let wings_right = line_char.repeat(wing_len);
        Line::from(vec![
            Span::styled(wings_left, theme.muted()),
            Span::styled(dot, theme.primary().add_modifier(Modifier::BOLD)),
            Span::styled(wings_right, theme.muted()),
        ])
    };

    let text_w = display_width(&line_content.to_string()) as u16;
    let start_x = area.x + area.width.saturating_sub(text_w) / 2;

    let target_rect = Rect::new(start_x, center_y, text_w.min(area.width), 1);
    frame.render_widget(
        Paragraph::new(line_content).alignment(Alignment::Center),
        target_rect,
    );
}

/// Renders Phase 2: Progressive reveal of TerminalVision identity.
fn render_identity(frame: &mut Frame, area: Rect, _local_p: f32, theme: &Theme, symbols: &Symbols) {
    let box_w = 48.min(area.width.saturating_sub(4));
    let box_h = 7.min(area.height.saturating_sub(2));
    let box_x = area.x + (area.width.saturating_sub(box_w)) / 2;
    let box_y = area.y + (area.height.saturating_sub(box_h)) / 2;
    let card_rect = Rect::new(box_x, box_y, box_w, box_h);

    let sep = if symbols.focus_bullet == "*" {
        "|"
    } else {
        "·"
    };
    let border_type = if symbols.focus_bullet == "*" {
        BorderType::Plain
    } else {
        BorderType::Rounded
    };

    let card_block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(theme.focus())
        .style(Style::default().bg(theme.palette.surface));

    let inner = card_block.inner(card_rect);
    frame.render_widget(card_block, card_rect);

    let title_line = Line::from(vec![Span::styled(
        "T E R M I N A L V I S I O N",
        theme.primary().add_modifier(Modifier::BOLD),
    )]);

    let subtitle_line = Line::from(vec![Span::styled(
        format!("SEE {sep} UNDERSTAND {sep} CONTROL"),
        theme.secondary(),
    )]);

    let text_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(title_line).alignment(Alignment::Center),
        text_layout[0],
    );
    frame.render_widget(
        Paragraph::new(subtitle_line).alignment(Alignment::Center),
        text_layout[2],
    );
}

/// Renders Phase 3: Real system readiness checklist.
fn render_system_readiness(
    frame: &mut Frame,
    area: Rect,
    boot_state: &BootState,
    _local_p: f32,
    theme: &Theme,
    symbols: &Symbols,
) {
    let box_w = 52.min(area.width.saturating_sub(4));
    let box_h = 12.min(area.height.saturating_sub(2));
    let box_x = area.x + (area.width.saturating_sub(box_w)) / 2;
    let box_y = area.y + (area.height.saturating_sub(box_h)) / 2;
    let card_rect = Rect::new(box_x, box_y, box_w, box_h);

    let check = symbols.check_mark;
    let dash = if symbols.focus_bullet == "*" {
        "-"
    } else {
        "—"
    };
    let border_type = if symbols.focus_bullet == "*" {
        BorderType::Plain
    } else {
        BorderType::Rounded
    };

    let card_block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(theme.focus())
        .title(Span::styled(
            " SYSTEM INITIALIZATION ",
            theme.primary().add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(theme.palette.surface));

    let inner = card_block.inner(card_rect);
    frame.render_widget(card_block, card_rect);

    let mut lines = Vec::new();

    // 1. Filesystem
    if boot_state.filesystem_ready {
        lines.push(Line::from(vec![
            Span::styled(format!(" {check} "), theme.success()),
            Span::styled("Filesystem       ", theme.primary()),
            Span::styled("READY", theme.success().add_modifier(Modifier::BOLD)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled(format!(" {dash} "), theme.warning()),
            Span::styled("Filesystem       ", theme.primary()),
            Span::styled("UNAVAILABLE", theme.warning()),
        ]));
    }

    // 2. Terminal
    if boot_state.terminal_ready {
        lines.push(Line::from(vec![
            Span::styled(format!(" {check} "), theme.success()),
            Span::styled("Terminal         ", theme.primary()),
            Span::styled("READY", theme.success().add_modifier(Modifier::BOLD)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled(format!(" {dash} "), theme.muted()),
            Span::styled("Terminal         ", theme.muted()),
            Span::styled("STANDBY", theme.muted()),
        ]));
    }

    // 3. Configuration
    if boot_state.config_ready {
        lines.push(Line::from(vec![
            Span::styled(format!(" {check} "), theme.success()),
            Span::styled("Configuration    ", theme.primary()),
            Span::styled("LOADED", theme.success().add_modifier(Modifier::BOLD)),
        ]));
    }

    // 4. Project
    if let Some(ref p_type) = boot_state.project_type {
        lines.push(Line::from(vec![
            Span::styled(format!(" {check} "), theme.success()),
            Span::styled("Project          ", theme.primary()),
            Span::styled(
                p_type.to_uppercase(),
                theme.primary().add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled(format!(" {dash} "), theme.muted()),
            Span::styled("Project          ", theme.muted()),
            Span::styled("NOT DETECTED", theme.muted()),
        ]));
    }

    // 5. Git
    if let Some(ref branch) = boot_state.git_branch {
        let dirty_suffix = if boot_state.git_dirty { "*" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(format!(" {check} "), theme.success()),
            Span::styled("Git              ", theme.primary()),
            Span::styled(
                format!("{branch}{dirty_suffix}"),
                theme.secondary().add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled(format!(" {dash} "), theme.muted()),
            Span::styled("Git              ", theme.muted()),
            Span::styled("NOT DETECTED", theme.muted()),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Renders Phase 4: Project awareness or workspace overview.
fn render_project_awareness(
    frame: &mut Frame,
    area: Rect,
    boot_state: &BootState,
    _local_p: f32,
    theme: &Theme,
    symbols: &Symbols,
) {
    let box_w = 54.min(area.width.saturating_sub(4));
    let box_h = 13.min(area.height.saturating_sub(2));
    let box_x = area.x + (area.width.saturating_sub(box_w)) / 2;
    let box_y = area.y + (area.height.saturating_sub(box_h)) / 2;
    let card_rect = Rect::new(box_x, box_y, box_w, box_h);

    let check = symbols.check_mark;
    let sep = if symbols.focus_bullet == "*" {
        "|"
    } else {
        "·"
    };
    let border_type = if symbols.focus_bullet == "*" {
        BorderType::Plain
    } else {
        BorderType::Rounded
    };

    let title_text = if boot_state.has_project() {
        " PROJECT DETECTED "
    } else {
        " WORKSPACE READY "
    };

    let card_block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(theme.focus())
        .title(Span::styled(
            title_text,
            theme.primary().add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(theme.palette.surface));

    let inner = card_block.inner(card_rect);
    frame.render_widget(card_block, card_rect);

    let mut lines = Vec::new();

    if boot_state.has_project() {
        let name = boot_state.project_name.as_deref().unwrap_or("Project");
        lines.push(Line::from(vec![Span::styled(
            format!(" {name}"),
            theme.primary().add_modifier(Modifier::BOLD),
        )]));

        let mut badges = Vec::new();
        if let Some(ref p_type) = boot_state.project_type {
            badges.push(p_type.as_str());
        }
        for ind in &boot_state.project_indicators {
            if !badges.contains(&ind.as_str()) {
                badges.push(ind.as_str());
            }
        }
        if boot_state.git_branch.is_some() && !badges.contains(&"Git") {
            badges.push("Git");
        }

        let badges_str = badges.join(&format!(" {sep} "));
        lines.push(Line::from(vec![Span::styled(
            format!(" {badges_str}"),
            theme.secondary(),
        )]));

        lines.push(Line::raw(""));

        // Structure items
        if !boot_state.project_structure_items.is_empty() {
            for (item_name, exists) in &boot_state.project_structure_items {
                if *exists {
                    let padding = 20usize.saturating_sub(display_width(item_name));
                    let pad_str = " ".repeat(padding);
                    lines.push(Line::from(vec![
                        Span::styled(format!(" {item_name}{pad_str}"), theme.primary()),
                        Span::styled(check, theme.success().add_modifier(Modifier::BOLD)),
                    ]));
                }
            }
        } else {
            lines.push(Line::from(vec![Span::styled(
                format!(" {check} Structure analyzed"),
                theme.success(),
            )]));
        }
    } else {
        // Non-project workspace (e.g. ~/Downloads)
        let path_display = boot_state.display_path();
        lines.push(Line::from(vec![Span::styled(
            format!(" {path_display}"),
            theme.primary().add_modifier(Modifier::BOLD),
        )]));

        lines.push(Line::from(vec![Span::styled(
            format!(" {} items loaded", boot_state.filesystem_entry_count),
            theme.secondary(),
        )]));

        lines.push(Line::raw(""));

        lines.push(Line::from(vec![
            Span::styled(format!(" {check} "), theme.success()),
            Span::styled("Filesystem Ready", theme.primary()),
        ]));

        lines.push(Line::from(vec![
            Span::styled(format!(" {check} "), theme.success()),
            Span::styled("Dual Panes Ready", theme.primary()),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Renders Phase 5: Interface Construction & signature Vision Pulse.
fn render_vision_pulse_phase(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    _boot_state: &BootState,
    local_p: f32,
    theme: &Theme,
    symbols: &Symbols,
) {
    // 1. Render the real underlying application layout
    let layout = crate::layout::geometry::ScreenLayout::calculate(area);

    if layout.header().height > 0 && layout.header().width > 0 {
        crate::ui::header::render(frame, layout.header(), app);
    }

    crate::ui::panes::render(frame, layout.main(), app);

    if layout.terminal().height > 0 && layout.terminal().width > 0 {
        let is_terminal_focused = app.mode() == crate::app::modes::Mode::Terminal;
        crate::ui::terminal::render(frame, layout.terminal(), app, is_terminal_focused);
    }

    if layout.footer().height > 0 && layout.footer().width > 0 {
        crate::ui::footer::render(frame, layout.footer(), app);
    }

    crate::ui::dialogs::render(frame, area, app);

    // 2. Overlay the signature Vision Pulse highlight across the header region
    if layout.header().height > 0 && layout.header().width > 0 {
        let h_rect = layout.header();
        let pulse_w = (h_rect.width as f32 * local_p).min(h_rect.width as f32) as u16;
        if pulse_w > 0 {
            let pulse_rect = Rect::new(h_rect.x, h_rect.y, pulse_w, 1);
            let bar_char = if symbols.focus_bullet == "*" {
                "="
            } else {
                "━"
            };
            let bar = bar_char.repeat(pulse_w as usize);
            frame.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(
                    bar,
                    theme.primary().add_modifier(Modifier::BOLD),
                )])),
                pulse_rect,
            );
        }
    }
}

/// Renders a compact, overflow-free fallback for very small terminal windows.
fn render_compact_fallback(
    frame: &mut Frame,
    area: Rect,
    _app: &App,
    boot_state: &BootState,
    phase: BootPhase,
    theme: &Theme,
    symbols: &Symbols,
) {
    frame.render_widget(Clear, area);
    let dot = symbols.focus_bullet;
    let sep = if symbols.focus_bullet == "*" {
        "|"
    } else {
        "·"
    };

    let mut lines = Vec::new();

    let title_line = Line::from(vec![
        Span::styled(
            format!("{dot} TERMINALVISION "),
            theme.primary().add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("[{:?}]", phase), theme.secondary()),
    ]);
    lines.push(title_line);

    if boot_state.has_project() {
        let p_name = boot_state.project_name.as_deref().unwrap_or("Project");
        let p_type = boot_state.project_type.as_deref().unwrap_or("Ready");
        lines.push(Line::from(vec![Span::styled(
            format!("{p_name} {sep} {p_type}"),
            theme.primary(),
        )]));
    } else {
        let path = boot_state.display_path();
        lines.push(Line::from(vec![Span::styled(
            format!("{path} ({} items)", boot_state.filesystem_entry_count),
            theme.primary(),
        )]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}
