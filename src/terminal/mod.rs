//! The integrated terminal layer.
//!
//! Owns the interactive shell PTY process, the ANSI terminal emulator,
//! keyboard input encoding, and output synchronization.

pub mod emulator;
pub mod pty;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub use emulator::{Cell, MAX_SCROLLBACK_LINES, TerminalEmulator};
pub use pty::PtySession;

/// An integrated interactive terminal session.
#[derive(Debug)]
pub struct TerminalSession {
    pty: PtySession,
    emulator: Arc<Mutex<TerminalEmulator>>,
    initial_cwd: PathBuf,
}

impl TerminalSession {
    /// Starts a new embedded terminal session at the directory `cwd` with dimensions `cols` by `rows`.
    pub fn start(cwd: &Path, cols: u16, rows: u16) -> io::Result<Self> {
        let pty = PtySession::spawn(cwd, cols, rows)?;
        let emulator = Arc::new(Mutex::new(TerminalEmulator::new(cols, rows)));

        Ok(Self {
            pty,
            emulator,
            initial_cwd: cwd.to_path_buf(),
        })
    }

    /// Reads all pending output from the PTY and updates the terminal emulator screen.
    pub fn poll_output(&self) -> bool {
        let bytes = self.pty.try_read_output();
        if !bytes.is_empty()
            && let Ok(mut emu) = self.emulator.lock()
        {
            emu.feed_bytes(&bytes);
            return true;
        }
        false
    }

    /// Writes raw bytes directly to the interactive shell.
    pub fn write_bytes(&self, bytes: &[u8]) -> io::Result<()> {
        self.pty.write_bytes(bytes)
    }

    /// Translates a keyboard event into terminal escape sequences and sends it to the PTY.
    pub fn send_key(&self, key: KeyEvent) -> io::Result<()> {
        if let Some(bytes) = encode_key_event(key) {
            self.write_bytes(&bytes)?;
            if let Ok(mut emu) = self.emulator.lock() {
                emu.reset_scroll();
            }
        }
        Ok(())
    }

    /// Resizes the PTY and emulator grid.
    pub fn resize(&self, cols: u16, rows: u16) {
        self.pty.resize(cols, rows);
        if let Ok(mut emu) = self.emulator.lock() {
            emu.resize(cols, rows);
        }
    }

    /// Current working directory of the shell (if tracked via OSC 7 or fallback to initial cwd).
    pub fn current_path(&self) -> PathBuf {
        if let Ok(emu) = self.emulator.lock()
            && let Some(cwd) = emu.tracked_cwd()
        {
            return cwd.clone();
        }
        self.initial_cwd.clone()
    }

    /// Synchronizes the shell to a directory by typing `cd "<path>"\n`.
    pub fn cd_to_path(&self, target: &Path) -> io::Result<()> {
        let cmd = format!(" cd \"{}\"\n", target.display());
        self.write_bytes(cmd.as_bytes())
    }

    /// Name of the shell executable (e.g. "zsh", "bash", "sh").
    pub fn shell_name(&self) -> &str {
        self.pty.shell_name()
    }

    /// Whether the underlying shell process is alive.
    pub fn is_alive(&self) -> bool {
        self.pty.is_alive()
    }

    /// Returns the snapshot of cells to render for the terminal window.
    pub fn visible_rows(&self) -> Vec<Vec<Cell>> {
        if let Ok(emu) = self.emulator.lock() {
            emu.visible_rows()
        } else {
            Vec::new()
        }
    }

    /// Returns cursor row, col, and visibility.
    pub fn cursor_info(&self) -> (u16, u16, bool) {
        if let Ok(emu) = self.emulator.lock() {
            (emu.cursor_row(), emu.cursor_col(), emu.cursor_visible())
        } else {
            (0, 0, false)
        }
    }

    /// Scrolls terminal history upward.
    pub fn scroll_up(&self, count: usize) {
        if let Ok(mut emu) = self.emulator.lock() {
            emu.scroll_up(count);
        }
    }

    /// Scrolls terminal history downward.
    pub fn scroll_down(&self, count: usize) {
        if let Ok(mut emu) = self.emulator.lock() {
            emu.scroll_down(count);
        }
    }

    /// Resets terminal scrollback to live view.
    pub fn reset_scroll(&self) {
        if let Ok(mut emu) = self.emulator.lock() {
            emu.reset_scroll();
        }
    }

    /// Terminates the child shell process cleanly.
    pub fn terminate(&self) {
        self.pty.terminate();
    }
}

/// Encodes a Crossterm [`KeyEvent`] into standard ANSI terminal escape byte sequence.
pub fn encode_key_event(key: KeyEvent) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                let upper = c.to_ascii_uppercase();
                match upper {
                    'A'..='Z' => {
                        let byte = (upper as u8) - b'A' + 1;
                        Some(vec![byte])
                    }
                    '@' | ' ' => Some(vec![0]),
                    '[' => Some(vec![27]),
                    '\\' => Some(vec![28]),
                    ']' => Some(vec![29]),
                    '^' => Some(vec![30]),
                    '_' => Some(vec![31]),
                    '?' => Some(vec![127]),
                    _ => None,
                }
            } else if alt {
                let mut bytes = vec![0x1b];
                let mut buf = [0u8; 4];
                bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                Some(bytes)
            } else {
                let mut buf = [0u8; 4];
                Some(c.encode_utf8(&mut buf).as_bytes().to_vec())
            }
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Tab => {
            if shift {
                Some(b"\x1b[Z".to_vec()) // BackTab
            } else {
                Some(vec![b'\t'])
            }
        }
        KeyCode::BackTab => Some(b"\x1b[Z".to_vec()),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::F(1) => Some(b"\x1bOP".to_vec()),
        KeyCode::F(2) => Some(b"\x1bOQ".to_vec()),
        KeyCode::F(3) => Some(b"\x1bOR".to_vec()),
        KeyCode::F(4) => Some(b"\x1bOS".to_vec()),
        KeyCode::F(5) => Some(b"\x1b[15~".to_vec()),
        KeyCode::F(6) => Some(b"\x1b[17~".to_vec()),
        KeyCode::F(7) => Some(b"\x1b[18~".to_vec()),
        KeyCode::F(8) => Some(b"\x1b[19~".to_vec()),
        KeyCode::F(9) => Some(b"\x1b[20~".to_vec()),
        KeyCode::F(10) => Some(b"\x1b[21~".to_vec()),
        KeyCode::F(11) => Some(b"\x1b[23~".to_vec()),
        KeyCode::F(12) => Some(b"\x1b[24~".to_vec()),
        _ => None,
    }
}
