//! Pseudo-terminal (PTY) interface for Unix platforms.
//!
//! Spawns the user's interactive shell inside a real pseudo-terminal pair,
//! handles non-blocking asynchronous I/O, window resizing via `TIOCSWINSZ`,
//! and clean process termination.

use std::ffi::CString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{FromRawFd, RawFd};

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;

/// A running PTY session connected to a child shell process.
pub struct PtySession {
    master_fd: RawFd,
    child_pid: libc::pid_t,
    writer: Arc<Mutex<File>>,
    output_rx: Receiver<Vec<u8>>,
    is_alive: Arc<AtomicBool>,
    shell_name: String,
}

impl std::fmt::Debug for PtySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtySession")
            .field("master_fd", &self.master_fd)
            .field("child_pid", &self.child_pid)
            .field("shell_name", &self.shell_name)
            .field("is_alive", &self.is_alive.load(Ordering::Relaxed))
            .finish()
    }
}

impl PtySession {
    /// Spawns a new interactive shell inside a PTY with initial `cols` and `rows`
    /// in the given working directory `cwd`.
    pub fn spawn(cwd: &Path, cols: u16, rows: u16) -> io::Result<Self> {
        let shell_path = detect_shell();
        let shell_name = Path::new(&shell_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("shell")
            .to_string();

        let mut master_fd: RawFd = -1;
        let mut slave_fd: RawFd = -1;

        let mut ws = libc::winsize {
            ws_row: rows.max(1),
            ws_col: cols.max(1),
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        // Open pseudo-terminal pair
        let res = unsafe {
            libc::openpty(
                &mut master_fd,
                &mut slave_fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut ws,
            )
        };

        if res != 0 {
            return Err(io::Error::last_os_error());
        }

        let c_cwd = CString::new(cwd.to_string_lossy().as_bytes())
            .unwrap_or_else(|_| CString::new(".").unwrap());
        let c_shell =
            CString::new(shell_path.clone()).unwrap_or_else(|_| CString::new("/bin/sh").unwrap());
        let c_arg0 =
            CString::new(format!("-{shell_name}")).unwrap_or_else(|_| CString::new("-sh").unwrap());

        let pid = unsafe { libc::fork() };

        if pid < 0 {
            unsafe {
                libc::close(master_fd);
                libc::close(slave_fd);
            }
            return Err(io::Error::last_os_error());
        }

        if pid == 0 {
            // Child process
            unsafe {
                libc::close(master_fd);

                // Create new session and become process group leader
                libc::setsid();

                // Set controlling terminal
                libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0);

                // Duplicate slave PTY to standard streams
                libc::dup2(slave_fd, libc::STDIN_FILENO);
                libc::dup2(slave_fd, libc::STDOUT_FILENO);
                libc::dup2(slave_fd, libc::STDERR_FILENO);

                if slave_fd > 2 {
                    libc::close(slave_fd);
                }

                // Change working directory
                let _ = libc::chdir(c_cwd.as_ptr());

                // Set terminal environment variables
                libc::setenv(c"TERM".as_ptr(), c"xterm-256color".as_ptr(), 1);
                libc::setenv(c"COLORTERM".as_ptr(), c"truecolor".as_ptr(), 1);

                let args = [c_arg0.as_ptr(), std::ptr::null()];
                libc::execvp(c_shell.as_ptr(), args.as_ptr());

                // If execvp fails, exit immediately
                libc::_exit(127);
            }
        }

        // Parent process
        unsafe {
            libc::close(slave_fd);
        }

        // Clone master fd for writer and background reader
        let master_file = unsafe { File::from_raw_fd(master_fd) };
        let mut reader_file = match master_file.try_clone() {
            Ok(f) => f,
            Err(e) => {
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                    libc::close(master_fd);
                }
                return Err(e);
            }
        };

        let writer = Arc::new(Mutex::new(master_file));
        let (tx, rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = channel();
        let is_alive = Arc::new(AtomicBool::new(true));
        let is_alive_clone = Arc::clone(&is_alive);

        // Spawn background reader thread
        thread::Builder::new()
            .name("tv-pty-reader".into())
            .spawn(move || {
                let mut buffer = [0u8; 8192];
                loop {
                    match reader_file.read(&mut buffer) {
                        Ok(0) => {
                            is_alive_clone.store(false, Ordering::SeqCst);
                            break;
                        }
                        Ok(n) => {
                            if tx.send(buffer[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            if e.kind() != io::ErrorKind::Interrupted
                                && e.kind() != io::ErrorKind::WouldBlock
                            {
                                is_alive_clone.store(false, Ordering::SeqCst);
                                break;
                            }
                        }
                    }
                }
            })
            .map_err(io::Error::other)?;

        Ok(Self {
            master_fd,
            child_pid: pid,
            writer,
            output_rx: rx,
            is_alive,
            shell_name,
        })
    }

    /// Returns non-blocking received bytes from the PTY output stream.
    pub fn try_read_output(&self) -> Vec<u8> {
        let mut result = Vec::new();
        while let Ok(chunk) = self.output_rx.try_recv() {
            result.extend(chunk);
        }
        result
    }

    /// Writes raw input bytes to the PTY (user keypresses / typed characters).
    pub fn write_bytes(&self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        if let Ok(mut writer) = self.writer.lock() {
            writer.write_all(bytes)?;
            writer.flush()?;
        }
        Ok(())
    }

    /// Resizes the PTY window geometry.
    pub fn resize(&self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 {
            return;
        }
        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe {
            libc::ioctl(self.master_fd, libc::TIOCSWINSZ as _, &ws);
        }
    }

    /// Name of the shell executable (e.g. "zsh", "bash", "sh").
    pub fn shell_name(&self) -> &str {
        &self.shell_name
    }

    /// Returns whether the child shell process is still active.
    pub fn is_alive(&self) -> bool {
        if !self.is_alive.load(Ordering::SeqCst) {
            return false;
        }
        let mut status = 0;
        let res = unsafe { libc::waitpid(self.child_pid, &mut status, libc::WNOHANG) };
        if res == self.child_pid {
            self.is_alive.store(false, Ordering::SeqCst);
            false
        } else {
            true
        }
    }

    /// Sends a termination signal and reaps the child shell.
    pub fn terminate(&self) {
        self.is_alive.store(false, Ordering::SeqCst);
        unsafe {
            libc::kill(self.child_pid, libc::SIGHUP);
            libc::kill(self.child_pid, libc::SIGTERM);
            let mut status = 0;
            libc::waitpid(self.child_pid, &mut status, libc::WNOHANG);
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// Detects the user's default shell from `$SHELL` or standard fallback paths.
fn detect_shell() -> String {
    if let Ok(shell) = std::env::var("SHELL")
        && !shell.trim().is_empty()
        && Path::new(&shell).exists()
    {
        return shell;
    }

    #[cfg(target_os = "macos")]
    {
        if Path::new("/bin/zsh").exists() {
            return "/bin/zsh".to_string();
        }
    }

    if Path::new("/bin/bash").exists() {
        return "/bin/bash".to_string();
    }
    if Path::new("/usr/bin/bash").exists() {
        return "/usr/bin/bash".to_string();
    }
    if Path::new("/bin/sh").exists() {
        return "/bin/sh".to_string();
    }

    "sh".to_string()
}
