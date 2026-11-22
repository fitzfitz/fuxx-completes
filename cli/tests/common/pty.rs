//! One pty core for the three shell harnesses: a temporary HOME, a clean environment, and
//! reading what the shell prints.

use expectrl::Session;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;

pub struct Pty {
    session: expectrl::session::OsSession,
    _home: TempDir,
    home_path: PathBuf,
    screen: Vec<u8>,
    /// Answer the terminal queries a shell waits on (fish 4: primary device attributes and the
    /// cursor position), as a terminal would.
    answer_queries: bool,
    answered: usize,
}

/// A fresh temporary HOME. The shell reports its own `$PWD`, which resolves symlinks
/// (/var -> /private/var on macOS); the resolved path is used everywhere so the two agree.
pub fn temp_home() -> (TempDir, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(home.path()).unwrap();
    (home, path)
}

/// `program` with an empty environment plus a temporary HOME, the `XDG_*` folders under it, a
/// plain `PATH`, `TERM` and `LANG`, started in HOME.
pub fn clean_command(program: &str, home: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(home)
        .env_clear()
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("PATH", "/usr/bin:/bin")
        .env("TERM", "xterm-256color")
        .env("LANG", "en_US.UTF-8");
    cmd
}

/// The core the adapter runs: a stub script written to `$HOME/core.sh` when given, else
/// `core_bin`, else the cargo-built `fuxx-completes`.
pub fn core_path(home: &Path, script: Option<&str>, bin: Option<PathBuf>) -> PathBuf {
    match script {
        Some(body) => Some(write_script(home, "core.sh", body)),
        None => bin,
    }
    .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_fuxx-completes")))
}

impl Pty {
    /// Starts `cmd` on a pty of `size` (columns, rows; default 120×40). `home` is kept alive
    /// (and removed) with the `Pty`.
    pub fn spawn(cmd: Command, size: Option<(u16, u16)>, home: (TempDir, PathBuf)) -> Pty {
        let mut session = Session::spawn(cmd).expect("spawn the shell");
        let (cols, rows) = size.unwrap_or((120, 40));
        session.get_process_mut().set_window_size(cols, rows).ok();
        Pty {
            session,
            _home: home.0,
            home_path: home.1,
            screen: Vec::new(),
            answer_queries: false,
            answered: 0,
        }
    }

    /// From now on, answer device attributes (`ESC [ c`, `ESC [ 0 c`, `ESC [ > c`) and cursor
    /// position (`ESC [ 6 n`) queries as a VT220-class terminal would.
    pub fn answer_queries(&mut self) {
        self.answer_queries = true;
    }

    fn answer(&mut self) {
        if !self.answer_queries {
            return;
        }
        const QUERIES: [(&[u8], &[u8]); 4] = [
            (b"\x1b[0c", b"\x1b[?62;22c"),
            (b"\x1b[c", b"\x1b[?62;22c"),
            (b"\x1b[>c", b"\x1b[>0;0;0c"),
            (b"\x1b[6n", b"\x1b[1;1R"),
        ];
        // A query counts once: when its last byte is past what the previous scan saw. Starting
        // a few bytes back catches one cut between two reads.
        let mut replies: Vec<&[u8]> = Vec::new();
        for start in self.answered.saturating_sub(4)..self.screen.len() {
            for (q, r) in QUERIES {
                let end = start + q.len();
                if end > self.answered && self.screen[start..].starts_with(q) {
                    replies.push(r);
                }
            }
        }
        self.answered = self.screen.len();
        for r in replies {
            self.session.write_all(r).unwrap();
        }
        self.session.flush().ok();
    }

    /// Reads whatever the shell has printed (non-blocking) into `screen`.
    pub fn pump(&mut self) {
        let mut buf = [0u8; 4096];
        loop {
            match self.session.try_read(&mut buf) {
                Ok(0) => break,
                Ok(n) => self.screen.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        self.answer();
    }

    /// Bytes read so far.
    pub fn len(&self) -> usize {
        self.screen.len()
    }

    /// Waits (5 s at most) until `needle` shows in the output, without sending a key.
    pub fn wait_screen(&mut self, needle: &str) -> bool {
        self.wait_for(0, needle, Duration::from_secs(5))
    }

    /// Polls until `needle` appears in the output at or after byte offset `from`.
    pub fn wait_for(&mut self, from: usize, needle: &str, within: Duration) -> bool {
        let end = Instant::now() + within;
        loop {
            self.pump();
            let tail = String::from_utf8_lossy(&self.screen[from.min(self.screen.len())..]);
            if tail.contains(needle) {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub fn send(&mut self, keys: &str) {
        self.send_bytes(keys.as_bytes());
    }

    pub fn send_bytes(&mut self, bytes: &[u8]) {
        self.session.write_all(bytes).unwrap();
        self.session.flush().unwrap();
    }

    /// Types `line`, presses Enter, and waits for the next `prompt` (10 s at most).
    pub fn run(&mut self, line: &str, prompt: &str) {
        self.pump();
        let mark = self.screen.len();
        self.send(&format!("{line}\r"));
        // The command's own line end comes first (a newline, or a cursor-down when rows such as
        // the suggestion list were drawn under the line); the next prompt after it ends the command.
        let end = Instant::now() + Duration::from_secs(10);
        let nl = loop {
            self.pump();
            let tail = String::from_utf8_lossy(&self.screen[mark..]).into_owned();
            let first = ["\r\n", "\n", "\u{1b}[1B"]
                .iter()
                .filter_map(|m| tail.find(m).map(|i| i + m.len()))
                .min();
            if let Some(i) = first {
                break Some(mark + i);
            }
            if Instant::now() >= end {
                break None;
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        let ok = nl.is_some_and(|nl| self.wait_for(nl, prompt, Duration::from_secs(10)));
        assert!(
            ok,
            "no prompt after {line:?}; output: {:?}",
            self.screen_since_start()
        );
    }

    /// All pty output so far, lossy UTF-8.
    pub fn screen_since_start(&self) -> String {
        String::from_utf8_lossy(&self.screen).into_owned()
    }

    /// The raw bytes read so far.
    pub fn bytes(&self) -> &[u8] {
        &self.screen
    }

    /// Like `screen_since_start`, after reading anything still pending: reads until two reads in
    /// a row, 10 ms apart, bring nothing.
    pub fn screen_now(&mut self) -> String {
        let mut quiet = 0;
        while quiet < 2 {
            let before = self.screen.len();
            self.pump();
            quiet = if self.screen.len() == before {
                quiet + 1
            } else {
                0
            };
            std::thread::sleep(Duration::from_millis(10));
        }
        self.screen_since_start()
    }

    pub fn home(&self) -> &Path {
        &self.home_path
    }
}

/// Writes an executable `/bin/sh` script into `dir` and returns its path.
pub fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}
