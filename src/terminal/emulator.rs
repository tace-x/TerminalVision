//! Terminal screen emulator and ANSI / VT100 sequence parser.
//!
//! Maintains a 2D cell grid, cursor positioning, drawing attributes (SGR colors
//! and styles), scrollback buffer, alternate screen buffer, and OSC 7 directory tracking.

use std::collections::VecDeque;
use std::path::PathBuf;

use ratatui::style::{Color, Modifier, Style};

/// Maximum number of scrollback lines retained in memory.
pub const MAX_SCROLLBACK_LINES: usize = 2000;

/// A single cell on the terminal screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: Style::default(),
        }
    }
}

impl Cell {
    pub const fn new(ch: char, style: Style) -> Self {
        Self { ch, style }
    }
}

/// An ANSI terminal emulator that parses byte streams and maintains a screen grid.
#[derive(Debug, Clone)]
pub struct TerminalEmulator {
    cols: u16,
    rows: u16,
    cursor_row: u16,
    cursor_col: u16,
    cursor_visible: bool,
    saved_cursor: (u16, u16),
    current_style: Style,

    // Main screen buffer
    main_grid: Vec<Vec<Cell>>,
    // Alternate screen buffer (for vim, nano, htop, less)
    alt_grid: Vec<Vec<Cell>>,
    is_alt_screen: bool,

    // Scrollback history (lines pushed off top of main grid)
    scrollback: VecDeque<Vec<Cell>>,
    scroll_offset: usize, // 0 = live view at bottom, >0 = scrolled up

    // OSC working directory detected from OSC 7 escape sequence
    tracked_cwd: Option<PathBuf>,

    // ANSI escape parser state machine
    parser_state: ParserState,
    param_buffer: String,
    intermediate: Vec<char>,
    osc_buffer: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParserState {
    Ground,
    Escape,
    Csi,
    Osc,
    Charset,
}

impl TerminalEmulator {
    /// Creates a new terminal emulator with the given column and row dimensions.
    pub fn new(cols: u16, rows: u16) -> Self {
        let c = cols.max(1);
        let r = rows.max(1);
        let main_grid = vec![vec![Cell::default(); c as usize]; r as usize];
        let alt_grid = vec![vec![Cell::default(); c as usize]; r as usize];

        Self {
            cols: c,
            rows: r,
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
            saved_cursor: (0, 0),
            current_style: Style::default(),
            main_grid,
            alt_grid,
            is_alt_screen: false,
            scrollback: VecDeque::with_capacity(MAX_SCROLLBACK_LINES),
            scroll_offset: 0,
            tracked_cwd: None,
            parser_state: ParserState::Ground,
            param_buffer: String::new(),
            intermediate: Vec::new(),
            osc_buffer: String::new(),
        }
    }

    /// Feeds incoming raw bytes from PTY output into the terminal emulator.
    pub fn feed_bytes(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        for ch in text.chars() {
            self.feed_char(ch);
        }
    }

    /// Feeds a single character into the escape state machine.
    pub fn feed_char(&mut self, ch: char) {
        match self.parser_state {
            ParserState::Ground => match ch {
                '\x1b' => {
                    self.parser_state = ParserState::Escape;
                }
                '\r' => {
                    self.cursor_col = 0;
                }
                '\n' => {
                    self.line_feed();
                }
                '\x08' => {
                    // Backspace
                    self.cursor_col = self.cursor_col.saturating_sub(1);
                }
                '\t' => {
                    // Tab to next multiple of 8
                    let next_tab = ((self.cursor_col / 8) + 1) * 8;
                    self.cursor_col = next_tab.min(self.cols.saturating_sub(1));
                }
                '\x07' | '\x0b' | '\x0c' => {
                    // Bell / form-feed
                }
                _ if !ch.is_control() => {
                    self.put_char(ch);
                }
                _ => {}
            },
            ParserState::Escape => match ch {
                '[' => {
                    self.parser_state = ParserState::Csi;
                    self.param_buffer.clear();
                    self.intermediate.clear();
                }
                ']' => {
                    self.parser_state = ParserState::Osc;
                    self.osc_buffer.clear();
                }
                '(' | ')' | '*' | '+' => {
                    self.parser_state = ParserState::Charset;
                }
                '7' => {
                    // Save cursor
                    self.saved_cursor = (self.cursor_row, self.cursor_col);
                    self.parser_state = ParserState::Ground;
                }
                '8' => {
                    // Restore cursor
                    self.cursor_row = self.saved_cursor.0.min(self.rows.saturating_sub(1));
                    self.cursor_col = self.saved_cursor.1.min(self.cols.saturating_sub(1));
                    self.parser_state = ParserState::Ground;
                }
                'M' => {
                    // Reverse index (scroll down if at top)
                    if self.cursor_row == 0 {
                        self.scroll_down_grid();
                    } else {
                        self.cursor_row = self.cursor_row.saturating_sub(1);
                    }
                    self.parser_state = ParserState::Ground;
                }
                'c' => {
                    // Reset terminal
                    self.reset();
                    self.parser_state = ParserState::Ground;
                }
                _ => {
                    self.parser_state = ParserState::Ground;
                }
            },
            ParserState::Charset => {
                // Ignore charset designator char and return to ground
                self.parser_state = ParserState::Ground;
            }
            ParserState::Csi => {
                if ch.is_ascii_digit() || ch == ';' || ch == '?' || ch == '>' {
                    self.param_buffer.push(ch);
                } else if ch == ' ' || ch == '\'' || ch == '"' {
                    self.intermediate.push(ch);
                } else {
                    self.execute_csi(ch);
                    self.parser_state = ParserState::Ground;
                }
            }
            ParserState::Osc => {
                if ch == '\x07' || ch == '\x1b' {
                    self.execute_osc();
                    self.parser_state = ParserState::Ground;
                } else {
                    self.osc_buffer.push(ch);
                }
            }
        }
    }

    /// Advances the cursor down one line, scrolling the grid if at the bottom.
    fn line_feed(&mut self) {
        if self.cursor_row + 1 < self.rows {
            self.cursor_row += 1;
        } else {
            self.scroll_up_grid();
        }
    }

    /// Scrolls the grid upward, saving the top line to scrollback if in main screen.
    fn scroll_up_grid(&mut self) {
        let grid = if self.is_alt_screen {
            &mut self.alt_grid
        } else {
            &mut self.main_grid
        };

        if !grid.is_empty() {
            let removed = grid.remove(0);
            if !self.is_alt_screen {
                if self.scrollback.len() >= MAX_SCROLLBACK_LINES {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(removed);
            }
            grid.push(vec![Cell::default(); self.cols as usize]);
        }
    }

    /// Scrolls the grid downward (inserting an empty line at the top).
    fn scroll_down_grid(&mut self) {
        let grid = if self.is_alt_screen {
            &mut self.alt_grid
        } else {
            &mut self.main_grid
        };

        if !grid.is_empty() {
            grid.pop();
            grid.insert(0, vec![Cell::default(); self.cols as usize]);
        }
    }

    /// Writes a printable character to the current cursor position.
    fn put_char(&mut self, ch: char) {
        let r = self.cursor_row as usize;
        let c = self.cursor_col as usize;

        let grid = if self.is_alt_screen {
            &mut self.alt_grid
        } else {
            &mut self.main_grid
        };

        if r < grid.len() && c < grid[r].len() {
            grid[r][c] = Cell::new(ch, self.current_style);
        }

        if self.cursor_col + 1 < self.cols {
            self.cursor_col += 1;
        } else {
            // Line wrap at edge
            self.cursor_col = 0;
            self.line_feed();
        }
    }

    /// Executes a CSI (Control Sequence Introducer) command.
    fn execute_csi(&mut self, cmd: char) {
        let is_private = self.param_buffer.starts_with('?');
        let param_str = if is_private {
            &self.param_buffer[1..]
        } else {
            &self.param_buffer
        };

        let params: Vec<u16> = param_str
            .split(';')
            .filter_map(|s| s.parse::<u16>().ok())
            .collect();

        match cmd {
            'A' => {
                // Cursor Up
                let count = params.first().copied().unwrap_or(1).max(1);
                self.cursor_row = self.cursor_row.saturating_sub(count);
            }
            'B' => {
                // Cursor Down
                let count = params.first().copied().unwrap_or(1).max(1);
                self.cursor_row = (self.cursor_row + count).min(self.rows.saturating_sub(1));
            }
            'C' => {
                // Cursor Forward / Right
                let count = params.first().copied().unwrap_or(1).max(1);
                self.cursor_col = (self.cursor_col + count).min(self.cols.saturating_sub(1));
            }
            'D' => {
                // Cursor Back / Left
                let count = params.first().copied().unwrap_or(1).max(1);
                self.cursor_col = self.cursor_col.saturating_sub(count);
            }
            'H' | 'f' => {
                // Cursor Position (1-indexed row;col)
                let r = params
                    .first()
                    .copied()
                    .unwrap_or(1)
                    .max(1)
                    .saturating_sub(1);
                let c = params.get(1).copied().unwrap_or(1).max(1).saturating_sub(1);
                self.cursor_row = r.min(self.rows.saturating_sub(1));
                self.cursor_col = c.min(self.cols.saturating_sub(1));
            }
            'G' => {
                // Cursor Horizontal Absolute
                let c = params
                    .first()
                    .copied()
                    .unwrap_or(1)
                    .max(1)
                    .saturating_sub(1);
                self.cursor_col = c.min(self.cols.saturating_sub(1));
            }
            'd' => {
                // Line Position Absolute (Row)
                let r = params
                    .first()
                    .copied()
                    .unwrap_or(1)
                    .max(1)
                    .saturating_sub(1);
                self.cursor_row = r.min(self.rows.saturating_sub(1));
            }
            'J' => {
                // Erase in Display
                let mode = params.first().copied().unwrap_or(0);
                self.erase_display(mode);
            }
            'K' => {
                // Erase in Line
                let mode = params.first().copied().unwrap_or(0);
                self.erase_line(mode);
            }
            'L' => {
                // Insert lines
                let count = params.first().copied().unwrap_or(1).max(1);
                for _ in 0..count {
                    let r = self.cursor_row as usize;
                    let grid = if self.is_alt_screen {
                        &mut self.alt_grid
                    } else {
                        &mut self.main_grid
                    };
                    if r < grid.len() {
                        grid.pop();
                        grid.insert(r, vec![Cell::default(); self.cols as usize]);
                    }
                }
            }
            'M' => {
                // Delete lines
                let count = params.first().copied().unwrap_or(1).max(1);
                for _ in 0..count {
                    let r = self.cursor_row as usize;
                    let grid = if self.is_alt_screen {
                        &mut self.alt_grid
                    } else {
                        &mut self.main_grid
                    };
                    if r < grid.len() {
                        grid.remove(r);
                        grid.push(vec![Cell::default(); self.cols as usize]);
                    }
                }
            }
            'P' => {
                // Delete characters
                let count = params.first().copied().unwrap_or(1).max(1) as usize;
                let r = self.cursor_row as usize;
                let c = self.cursor_col as usize;
                let grid = if self.is_alt_screen {
                    &mut self.alt_grid
                } else {
                    &mut self.main_grid
                };
                if r < grid.len() && c < grid[r].len() {
                    let line = &mut grid[r];
                    for _ in 0..count {
                        if c < line.len() {
                            line.remove(c);
                            line.push(Cell::default());
                        }
                    }
                }
            }
            'm' => {
                // Select Graphic Rendition (SGR)
                self.apply_sgr(&params);
            }
            'h' if is_private => {
                // Private mode set
                for &p in &params {
                    match p {
                        25 => self.cursor_visible = true,
                        47 | 1047 | 1049 => self.is_alt_screen = true,
                        _ => {}
                    }
                }
            }
            'l' if is_private => {
                // Private mode reset
                for &p in &params {
                    match p {
                        25 => self.cursor_visible = false,
                        47 | 1047 | 1049 => self.is_alt_screen = false,
                        _ => {}
                    }
                }
            }
            's' => {
                self.saved_cursor = (self.cursor_row, self.cursor_col);
            }
            'u' => {
                self.cursor_row = self.saved_cursor.0.min(self.rows.saturating_sub(1));
                self.cursor_col = self.saved_cursor.1.min(self.cols.saturating_sub(1));
            }
            _ => {}
        }
    }

    /// Erases in display (J command).
    fn erase_display(&mut self, mode: u16) {
        let grid = if self.is_alt_screen {
            &mut self.alt_grid
        } else {
            &mut self.main_grid
        };

        match mode {
            0 => {
                // Clear from cursor to end
                let r = self.cursor_row as usize;
                let c = self.cursor_col as usize;
                if r < grid.len() {
                    for cell in grid[r].iter_mut().skip(c) {
                        *cell = Cell::default();
                    }
                    for row in grid.iter_mut().skip(r + 1) {
                        for cell in row {
                            *cell = Cell::default();
                        }
                    }
                }
            }
            1 => {
                // Clear from start to cursor
                let r = self.cursor_row as usize;
                let c = self.cursor_col as usize;
                for row in 0..r.min(grid.len()) {
                    for cell in &mut grid[row] {
                        *cell = Cell::default();
                    }
                }
                if r < grid.len() {
                    for col in 0..=c.min(grid[r].len().saturating_sub(1)) {
                        grid[r][col] = Cell::default();
                    }
                }
            }
            2 => {
                // Clear entire screen
                for row in grid {
                    for cell in row {
                        *cell = Cell::default();
                    }
                }
            }
            3 => {
                // Clear screen and scrollback
                for row in grid {
                    for cell in row {
                        *cell = Cell::default();
                    }
                }
                self.scrollback.clear();
                self.scroll_offset = 0;
            }
            _ => {}
        }
    }

    /// Erases in line (K command).
    fn erase_line(&mut self, mode: u16) {
        let r = self.cursor_row as usize;
        let c = self.cursor_col as usize;
        let grid = if self.is_alt_screen {
            &mut self.alt_grid
        } else {
            &mut self.main_grid
        };

        if r < grid.len() {
            let line = &mut grid[r];
            match mode {
                0 => {
                    // Clear from cursor to end of line
                    for cell in line.iter_mut().skip(c) {
                        *cell = Cell::default();
                    }
                }
                1 => {
                    // Clear from start to cursor
                    for col in 0..=c.min(line.len().saturating_sub(1)) {
                        line[col] = Cell::default();
                    }
                }
                2 => {
                    // Clear entire line
                    for cell in line {
                        *cell = Cell::default();
                    }
                }
                _ => {}
            }
        }
    }

    /// Applies SGR color and modifier parameters.
    fn apply_sgr(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.current_style = Style::default();
            return;
        }

        let mut idx = 0;
        while idx < params.len() {
            match params[idx] {
                0 => self.current_style = Style::default(),
                1 => self.current_style = self.current_style.add_modifier(Modifier::BOLD),
                2 => self.current_style = self.current_style.add_modifier(Modifier::DIM),
                3 => self.current_style = self.current_style.add_modifier(Modifier::ITALIC),
                4 => self.current_style = self.current_style.add_modifier(Modifier::UNDERLINED),
                7 => self.current_style = self.current_style.add_modifier(Modifier::REVERSED),
                22 => {
                    self.current_style = self
                        .current_style
                        .remove_modifier(Modifier::BOLD)
                        .remove_modifier(Modifier::DIM);
                }
                23 => {
                    self.current_style = self.current_style.remove_modifier(Modifier::ITALIC);
                }
                24 => {
                    self.current_style = self.current_style.remove_modifier(Modifier::UNDERLINED);
                }
                27 => {
                    self.current_style = self.current_style.remove_modifier(Modifier::REVERSED);
                }
                // Standard Foreground Colors
                30 => self.current_style = self.current_style.fg(Color::Black),
                31 => self.current_style = self.current_style.fg(Color::Red),
                32 => self.current_style = self.current_style.fg(Color::Green),
                33 => self.current_style = self.current_style.fg(Color::Yellow),
                34 => self.current_style = self.current_style.fg(Color::Blue),
                35 => self.current_style = self.current_style.fg(Color::Magenta),
                36 => self.current_style = self.current_style.fg(Color::Cyan),
                37 => self.current_style = self.current_style.fg(Color::White),
                39 => self.current_style = self.current_style.fg(Color::Reset),
                // Standard Background Colors
                40 => self.current_style = self.current_style.bg(Color::Black),
                41 => self.current_style = self.current_style.bg(Color::Red),
                42 => self.current_style = self.current_style.bg(Color::Green),
                43 => self.current_style = self.current_style.bg(Color::Yellow),
                44 => self.current_style = self.current_style.bg(Color::Blue),
                45 => self.current_style = self.current_style.bg(Color::Magenta),
                46 => self.current_style = self.current_style.bg(Color::Cyan),
                47 => self.current_style = self.current_style.bg(Color::White),
                49 => self.current_style = self.current_style.bg(Color::Reset),
                // High-Intensity Foreground Colors
                90 => self.current_style = self.current_style.fg(Color::DarkGray),
                91 => self.current_style = self.current_style.fg(Color::LightRed),
                92 => self.current_style = self.current_style.fg(Color::LightGreen),
                93 => self.current_style = self.current_style.fg(Color::LightYellow),
                94 => self.current_style = self.current_style.fg(Color::LightBlue),
                95 => self.current_style = self.current_style.fg(Color::LightMagenta),
                96 => self.current_style = self.current_style.fg(Color::LightCyan),
                97 => self.current_style = self.current_style.fg(Color::White),
                // High-Intensity Background Colors
                100 => self.current_style = self.current_style.bg(Color::DarkGray),
                101 => self.current_style = self.current_style.bg(Color::LightRed),
                102 => self.current_style = self.current_style.bg(Color::LightGreen),
                103 => self.current_style = self.current_style.bg(Color::LightYellow),
                104 => self.current_style = self.current_style.bg(Color::LightBlue),
                105 => self.current_style = self.current_style.bg(Color::LightMagenta),
                106 => self.current_style = self.current_style.bg(Color::LightCyan),
                107 => self.current_style = self.current_style.bg(Color::White),
                // 256 / 24-bit Colors
                38 => {
                    if idx + 2 < params.len() && params[idx + 1] == 5 {
                        let color_idx = params[idx + 2] as u8;
                        self.current_style = self.current_style.fg(Color::Indexed(color_idx));
                        idx += 2;
                    } else if idx + 4 < params.len() && params[idx + 1] == 2 {
                        let r = params[idx + 2] as u8;
                        let g = params[idx + 3] as u8;
                        let b = params[idx + 4] as u8;
                        self.current_style = self.current_style.fg(Color::Rgb(r, g, b));
                        idx += 4;
                    }
                }
                48 => {
                    if idx + 2 < params.len() && params[idx + 1] == 5 {
                        let color_idx = params[idx + 2] as u8;
                        self.current_style = self.current_style.bg(Color::Indexed(color_idx));
                        idx += 2;
                    } else if idx + 4 < params.len() && params[idx + 1] == 2 {
                        let r = params[idx + 2] as u8;
                        let g = params[idx + 3] as u8;
                        let b = params[idx + 4] as u8;
                        self.current_style = self.current_style.bg(Color::Rgb(r, g, b));
                        idx += 4;
                    }
                }
                _ => {}
            }
            idx += 1;
        }
    }

    /// Executes OSC commands (e.g. OSC 7 for CWD tracking).
    fn execute_osc(&mut self) {
        if self.osc_buffer.starts_with("7;") {
            let uri = &self.osc_buffer[2..];
            if let Some(path_part) = uri.strip_prefix("file://")
                && let Some(slash_idx) = path_part.find('/')
            {
                let path_str = &path_part[slash_idx..];
                self.tracked_cwd = Some(PathBuf::from(path_str));
            }
        }
    }

    /// Resets the terminal emulator state.
    pub fn reset(&mut self) {
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.cursor_visible = true;
        self.saved_cursor = (0, 0);
        self.current_style = Style::default();
        self.main_grid = vec![vec![Cell::default(); self.cols as usize]; self.rows as usize];
        self.alt_grid = vec![vec![Cell::default(); self.cols as usize]; self.rows as usize];
        self.is_alt_screen = false;
        self.scrollback.clear();
        self.scroll_offset = 0;
    }

    /// Resizes the emulator grid to new columns and rows.
    pub fn resize(&mut self, new_cols: u16, new_rows: u16) {
        let cols = new_cols.max(1);
        let rows = new_rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }

        self.cols = cols;
        self.rows = rows;
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));

        resize_grid(&mut self.main_grid, cols, rows);
        resize_grid(&mut self.alt_grid, cols, rows);
    }

    /// Returns the visible lines of cells to render, respecting scrollback offset.
    pub fn visible_rows(&self) -> Vec<Vec<Cell>> {
        let height = self.rows as usize;
        let width = self.cols as usize;

        if self.is_alt_screen || self.scroll_offset == 0 {
            // Render directly from the active grid
            let grid = if self.is_alt_screen {
                &self.alt_grid
            } else {
                &self.main_grid
            };
            grid.clone()
        } else {
            // Viewing scrollback history
            let mut result = Vec::with_capacity(height);
            let total_history = self.scrollback.len();
            let effective_offset = self.scroll_offset.min(total_history);

            let start_idx = total_history.saturating_sub(effective_offset);
            for idx in start_idx..(start_idx + height).min(total_history) {
                let mut row = self.scrollback[idx].clone();
                row.resize(width, Cell::default());
                result.push(row);
            }

            // If history doesn't fill the full height, pull remaining from top of main_grid
            let remaining = height.saturating_sub(result.len());
            for idx in 0..remaining.min(self.main_grid.len()) {
                let mut row = self.main_grid[idx].clone();
                row.resize(width, Cell::default());
                result.push(row);
            }

            result
        }
    }

    /// Scrolls the scrollback view upward by `count` rows.
    pub fn scroll_up(&mut self, count: usize) {
        if !self.is_alt_screen {
            self.scroll_offset = (self.scroll_offset + count).min(self.scrollback.len());
        }
    }

    /// Scrolls the scrollback view downward by `count` rows.
    pub fn scroll_down(&mut self, count: usize) {
        if !self.is_alt_screen {
            self.scroll_offset = self.scroll_offset.saturating_sub(count);
        }
    }

    /// Resets scrollback view to the bottom live terminal screen.
    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }

    /// Cursor row (0-indexed).
    pub fn cursor_row(&self) -> u16 {
        self.cursor_row
    }

    /// Cursor column (0-indexed).
    pub fn cursor_col(&self) -> u16 {
        self.cursor_col
    }

    /// Whether cursor should be visibly drawn.
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible && self.scroll_offset == 0
    }

    /// Current working directory reported by shell via OSC 7.
    pub fn tracked_cwd(&self) -> Option<&PathBuf> {
        self.tracked_cwd.as_ref()
    }
}

/// Helper to resize a 2D cell grid to `cols` by `rows`.
fn resize_grid(grid: &mut Vec<Vec<Cell>>, cols: u16, rows: u16) {
    grid.resize(rows as usize, vec![Cell::default(); cols as usize]);
    for row in grid.iter_mut() {
        row.resize(cols as usize, Cell::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emulator_initialization() {
        let emu = TerminalEmulator::new(80, 24);
        assert_eq!(emu.cursor_row(), 0);
        assert_eq!(emu.cursor_col(), 0);
        assert_eq!(emu.visible_rows().len(), 24);
        assert_eq!(emu.visible_rows()[0].len(), 80);
    }

    #[test]
    fn test_basic_text_and_newline() {
        let mut emu = TerminalEmulator::new(80, 24);
        emu.feed_bytes(b"Hello World\r\nLine 2");
        assert_eq!(emu.cursor_row(), 1);
        assert_eq!(emu.cursor_col(), 6);

        let rows = emu.visible_rows();
        let line0: String = rows[0].iter().map(|c| c.ch).collect();
        assert!(line0.starts_with("Hello World"));
        let line1: String = rows[1].iter().map(|c| c.ch).collect();
        assert!(line1.starts_with("Line 2"));
    }

    #[test]
    fn test_ansi_color_sgr() {
        let mut emu = TerminalEmulator::new(80, 24);
        emu.feed_bytes(b"\x1b[31mRed\x1b[0mNormal");
        let rows = emu.visible_rows();
        assert_eq!(rows[0][0].ch, 'R');
        assert_eq!(rows[0][0].style.fg, Some(Color::Red));
        assert_eq!(rows[0][3].ch, 'N');
        assert_eq!(rows[0][3].style.fg, None);
    }

    #[test]
    fn test_cursor_movement_csi() {
        let mut emu = TerminalEmulator::new(80, 24);
        emu.feed_bytes(b"\x1b[5;10HTest");
        assert_eq!(emu.cursor_row(), 4);
        assert_eq!(emu.cursor_col(), 13);
    }

    #[test]
    fn test_scrollback_history() {
        let mut emu = TerminalEmulator::new(40, 4);
        for i in 1..=10 {
            emu.feed_bytes(format!("Line {i}\r\n").as_bytes());
        }
        assert_eq!(emu.scrollback.len(), 7);
        emu.scroll_up(3);
        assert_eq!(emu.scroll_offset, 3);
        emu.reset_scroll();
        assert_eq!(emu.scroll_offset, 0);
    }
}
