use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::app::AppEvent;
use crate::vt::parser::{Terminal, TerminalEvent};

/// A terminal pane wrapping a PTY and the in-house vt::Terminal parser.
pub struct Pane {
    pub id: usize,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    pub terminal: Arc<Mutex<Terminal>>,
    child: Box<dyn Child + Send + Sync>,
    _reader_handle: thread::JoinHandle<()>,
    last_rows: u16,
    last_cols: u16,
    pub exited: bool,
    pub title: Arc<Mutex<String>>,
    pub cwd: PathBuf,
    /// UI-side scroll offset into the scrollback. 0 = live (bottom). Larger
    /// values move the view back into history.
    pub scroll_offset: Arc<std::sync::atomic::AtomicUsize>,
}

impl Pane {
    /// Create a new pane with a PTY shell.
    pub fn new(id: usize, rows: u16, cols: u16, event_tx: Sender<AppEvent>) -> Result<Self> {
        Self::new_with_cwd(id, rows, cols, event_tx, None)
    }

    pub fn new_with_cwd(id: usize, rows: u16, cols: u16, event_tx: Sender<AppEvent>, cwd: Option<PathBuf>) -> Result<Self> {
        let pty_system = native_pty_system();

        let pty_size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(pty_size)
            .context("Failed to open PTY")?;

        let shell = detect_shell();
        let mut cmd = CommandBuilder::new(&shell);

        let shell_name = shell
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if shell_name.contains("bash") || shell_name.contains("zsh") {
            cmd.arg("--login");
        }

        let work_dir = cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        cmd.cwd(&work_dir);
        cmd.env("TERM", "xterm-256color");
        cmd.env("CCMUX", "1"); // marker to detect nested ccmux

        let child = pair
            .slave
            .spawn_command(cmd)
            .context("Failed to spawn shell")?;

        // Drop the slave side — we only use master
        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .context("Failed to take PTY writer")?;

        // Scrollback buffer: 10000 lines of history
        let terminal = Arc::new(Mutex::new(Terminal::new(rows, cols, 10000)));
        let pane_title = Arc::new(Mutex::new(String::new()));

        let reader = pair
            .master
            .try_clone_reader()
            .context("Failed to clone PTY reader")?;

        let terminal_clone = Arc::clone(&terminal);
        let title_clone = Arc::clone(&pane_title);
        let scroll_offset = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let reader_handle = thread::spawn(move || {
            pty_reader_thread(reader, terminal_clone, title_clone, id, event_tx);
        });

        let mut pane = Self {
            id,
            master: pair.master,
            writer,
            terminal,
            child,
            _reader_handle: reader_handle,
            last_rows: rows,
            last_cols: cols,
            exited: false,
            title: pane_title,
            cwd: work_dir,
            scroll_offset,
        };

        // Inject OSC 7 hook after shell starts
        // Leading space prevents it from appearing in bash history
        if shell_name.contains("bash") {
            let setup = concat!(
                " __ccmux_osc7() { printf '\\033]7;file://%s%s\\007' \"$HOSTNAME\" \"$PWD\"; };",
                " PROMPT_COMMAND=\"__ccmux_osc7;${PROMPT_COMMAND}\";",
                " clear\n",
            );
            let _ = pane.write_input(setup.as_bytes());
        } else if shell_name.contains("zsh") {
            let setup = concat!(
                " __ccmux_osc7() { printf '\\033]7;file://%s%s\\007' \"$HOST\" \"$PWD\"; };",
                " precmd_functions+=(__ccmux_osc7);",
                " clear\n",
            );
            let _ = pane.write_input(setup.as_bytes());
        }

        Ok(pane)
    }

    /// Write input bytes to the PTY (keyboard input from user).
    pub fn write_input(&mut self, data: &[u8]) -> Result<()> {
        if self.exited {
            return Ok(());
        }
        if self.writer.write_all(data).is_err() || self.writer.flush().is_err() {
            self.exited = true;
        }
        Ok(())
    }

    /// Resize the PTY and vt100 parser. Returns `true` if the size
    /// actually changed (useful for callers that want to know whether
    /// a SIGWINCH was sent to the child). No-op and returns `false`
    /// when the size hasn't changed.
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<bool> {
        if rows == 0 || cols == 0 {
            return Ok(false);
        }

        // Skip if size hasn't changed
        if rows == self.last_rows && cols == self.last_cols {
            return Ok(false);
        }

        self.last_rows = rows;
        self.last_cols = cols;

        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to resize PTY")?;

        let mut term = self.terminal.lock().unwrap_or_else(|e| e.into_inner());
        term.resize(rows, cols);
        // Clear the screen buffer to avoid rendering stale content at the new size.
        // The TUI app (e.g. Claude Code) receives SIGWINCH and will redraw.
        // A brief blank frame is preferable to overlapping garbled output.
        term.process(b"\x1b[2J\x1b[H");

        Ok(true)
    }

    fn max_scroll(&self) -> usize {
        let term = self.terminal.lock().unwrap_or_else(|e| e.into_inner());
        term.grid.current_buffer().scrollback.len()
    }

    /// Scroll the terminal view up (into scrollback history).
    pub fn scroll_up(&self, lines: usize) {
        let max = self.max_scroll();
        let cur = self.scroll_offset.load(std::sync::atomic::Ordering::Relaxed);
        self.scroll_offset
            .store((cur + lines).min(max), std::sync::atomic::Ordering::Relaxed);
    }

    /// Get scrollbar info: (current_offset, total_lines).
    pub fn scrollbar_info(&self) -> (usize, usize) {
        let term = self.terminal.lock().unwrap_or_else(|e| e.into_inner());
        let total = term.grid.current_buffer().scrollback.len() + term.grid.rows as usize;
        let current = self.scroll_offset.load(std::sync::atomic::Ordering::Relaxed);
        (current, total)
    }

    /// Scroll the terminal view down (towards current output).
    pub fn scroll_down(&self, lines: usize) {
        let cur = self.scroll_offset.load(std::sync::atomic::Ordering::Relaxed);
        self.scroll_offset
            .store(cur.saturating_sub(lines), std::sync::atomic::Ordering::Relaxed);
    }

    /// Reset scroll to the bottom (live view).
    pub fn scroll_reset(&self) {
        self.scroll_offset
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set scroll offset directly (clamped to current scrollback length).
    pub fn set_scroll_offset(&self, target: usize) {
        let max = self.max_scroll();
        self.scroll_offset
            .store(target.min(max), std::sync::atomic::Ordering::Relaxed);
    }

    /// Check if the terminal is scrolled back.
    pub fn is_scrolled_back(&self) -> bool {
        self.scroll_offset.load(std::sync::atomic::Ordering::Relaxed) > 0
    }

    /// Check if the PTY application has enabled bracketed paste mode.
    pub fn is_bracketed_paste_enabled(&self) -> bool {
        let term = self.terminal.lock().unwrap_or_else(|e| e.into_inner());
        term.grid.modes.bracketed_paste
    }

    /// Current window title (set by OSC 0/2). Empty when none was sent.
    #[allow(dead_code)]
    pub fn title(&self) -> String {
        self.title.lock().map(|t| t.clone()).unwrap_or_default()
    }

    /// Check if Claude Code is running in this pane (by window title).
    pub fn is_claude_running(&self) -> bool {
        if let Ok(t) = self.title.lock() {
            let lower = t.to_lowercase();
            lower.contains("claude")
        } else {
            false
        }
    }

    /// Kill the PTY child process.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Background thread that reads PTY output and feeds it to vt::Terminal.
fn pty_reader_thread(
    mut reader: Box<dyn Read + Send>,
    terminal: Arc<Mutex<Terminal>>,
    title: Arc<Mutex<String>>,
    pane_id: usize,
    event_tx: Sender<AppEvent>,
) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                let _ = event_tx.send(AppEvent::PtyEof(pane_id));
                break;
            }
            Ok(n) => {
                let data = &buf[..n];

                let events = {
                    let mut term = terminal.lock().unwrap_or_else(|e| e.into_inner());
                    term.process(data);
                    term.drain_events()
                };

                for ev in events {
                    match ev {
                        TerminalEvent::TitleChanged(new_title) => {
                            if let Ok(mut t) = title.lock() {
                                *t = new_title;
                            }
                        }
                        TerminalEvent::CwdChanged(path) => {
                            let _ = event_tx.send(AppEvent::CwdChanged(pane_id, path));
                        }
                        TerminalEvent::Bell
                        | TerminalEvent::ClipboardWrite(_)
                        | TerminalEvent::ClipboardReadRequested => {
                            // F-gate features not wired yet
                        }
                    }
                }

                let _ = event_tx.send(AppEvent::PtyOutput(pane_id));
            }
            Err(_) => {
                break;
            }
        }
    }
}

/// Detect the appropriate shell to launch.
pub fn detect_shell() -> PathBuf {
    #[cfg(windows)]
    {
        detect_shell_windows()
    }
    #[cfg(not(windows))]
    {
        detect_shell_unix()
    }
}

#[cfg(windows)]
fn detect_shell_windows() -> PathBuf {
    // Try Git Bash first
    let git_bash_paths = [
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files (x86)\Git\bin\bash.exe",
    ];

    for path in &git_bash_paths {
        let p = PathBuf::from(path);
        if p.exists() {
            return p;
        }
    }

    // Try bash in PATH
    if let Ok(output) = std::process::Command::new("where")
        .arg("bash")
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = stdout.lines().next() {
                let p = PathBuf::from(line.trim());
                if p.exists() {
                    return p;
                }
            }
        }
    }

    // Fallback to PowerShell
    PathBuf::from("powershell.exe")
}

#[cfg(not(windows))]
fn detect_shell_unix() -> PathBuf {
    if let Ok(shell) = std::env::var("SHELL") {
        let p = PathBuf::from(&shell);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("/bin/sh")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_shell_returns_valid_path() {
        let shell = detect_shell();
        assert!(
            !shell.as_os_str().is_empty(),
            "Shell path should not be empty"
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_detect_shell_windows_returns_exe() {
        let shell = detect_shell();
        let ext = shell
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase());
        assert_eq!(ext.as_deref(), Some("exe"), "Windows shell should be .exe");
    }

    #[cfg(not(windows))]
    #[test]
    fn test_detect_shell_unix_uses_shell_env() {
        let shell = detect_shell();
        if let Ok(env_shell) = std::env::var("SHELL") {
            assert_eq!(
                shell,
                PathBuf::from(&env_shell),
                "Should use $SHELL env var"
            );
        }
    }
}
