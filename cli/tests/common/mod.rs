//! A pty harness that drives a real interactive zsh with the adapter loaded.
//! Everything lives under a temporary HOME; the shell gets an empty environment plus a few
//! variables, so no real rc file, history or data is ever read or written.
#![allow(dead_code)] // Tasks 7-9 use the parts Task 6 doesn't.

use expectrl::Session;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const PROMPT: &str = "READY> ";
const US: char = '\u{1f}';
const DUMP_KEY: &str = "\x1b[9999~";

pub fn zsh_path() -> String {
    std::env::var("FUXX_TEST_ZSH").unwrap_or_else(|_| "zsh".into())
}

/// True when the zsh under test is at least version `v` (asked of zsh itself, with `is-at-least`).
pub fn zsh_at_least(v: &str) -> bool {
    Command::new(zsh_path())
        .args(["-fc", &format!("autoload is-at-least; is-at-least {v}")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[derive(Default)]
pub struct ZshOpts {
    pub history: Vec<&'static str>,
    pub rc_before: String,
    pub rc_after: String,
    /// The core binary; default is the cargo-built `fuxx-completes`.
    pub core_bin: Option<PathBuf>,
    /// A stub core: this `/bin/sh` script is written to `$HOME/core.sh` and used as the core
    /// (wins over `core_bin`). The recorder is `exec cat > "$HOME/wire.log"`.
    pub core_script: Option<&'static str>,
}

#[derive(Default, Debug, Clone)]
pub struct LineState {
    pub buffer: String,
    pub cursor: usize,
    pub postdisplay: String,
    pub region_highlight: Vec<String>,
    /// `$_FUXX_AS_PID`
    pub core_pid: Option<u32>,
}

pub struct ZshHarness {
    session: expectrl::session::OsSession,
    home: TempDir,
    home_path: PathBuf,
    screen: Vec<u8>,
}

impl ZshHarness {
    pub fn start(opts: ZshOpts) -> Self {
        let home = tempfile::tempdir().unwrap();
        // The shell reports its own `$PWD`, which resolves symlinks (/var -> /private/var on
        // macOS); use the resolved path everywhere so the two agree.
        let home_path = std::fs::canonicalize(home.path()).unwrap();

        let mut hist = String::new();
        for entry in &opts.history {
            hist.push_str(&entry.replace('\n', "\\\n"));
            hist.push('\n');
        }
        std::fs::write(home_path.join(".zsh_history"), hist).unwrap();

        let plugin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../adapters/zsh/fuxx-autosuggestions.plugin.zsh");
        let rc = format!(
            "{before}\n\
             HISTFILE=$HOME/.zsh_history; HISTSIZE=1000; SAVEHIST=1000; PS1='{PROMPT}'; bindkey -e; KEYTIMEOUT=1\n\
             source '{plugin}'\n\
             _fuxx-test-dump() {{\n\
               emulate -L zsh\n\
               print -rn -- \"$BUFFER\"$'\\x1f'\"$CURSOR\"$'\\x1f'\"$POSTDISPLAY\"$'\\x1f'\"${{(pj:\\x1e:)region_highlight}}\"$'\\x1f'\"${{_FUXX_AS_PID-}}\"$'\\x1f'END >| \"$HOME/dump\"\n\
             }}\n\
             zle -N _fuxx-test-dump\n\
             bindkey '^[[9999~' _fuxx-test-dump\n\
             {after}\n",
            before = opts.rc_before,
            plugin = plugin.display(),
            after = opts.rc_after,
        );
        std::fs::write(home_path.join(".zshrc"), rc).unwrap();

        let core = match opts.core_script {
            Some(body) => Some(write_script(&home_path, "core.sh", body)),
            None => opts.core_bin,
        }
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_fuxx-completes")));
        let mut cmd = Command::new(zsh_path());
        cmd.args(["-d", "-i"])
            .current_dir(&home_path)
            .env_clear()
            .env("HOME", &home_path)
            .env("ZDOTDIR", &home_path)
            .env("XDG_DATA_HOME", home_path.join(".local/share"))
            .env("XDG_CONFIG_HOME", home_path.join(".config"))
            .env("XDG_CACHE_HOME", home_path.join(".cache"))
            .env("XDG_STATE_HOME", home_path.join(".local/state"))
            .env("PATH", "/usr/bin:/bin")
            .env("TERM", "xterm-256color")
            .env("LANG", "en_US.UTF-8")
            .env("FUXX_COMPLETES_BIN", core);
        let mut session = Session::spawn(cmd).expect("spawn zsh");
        session.get_process_mut().set_window_size(120, 40).ok();

        let mut z = ZshHarness {
            session,
            home,
            home_path,
            screen: Vec::new(),
        };
        assert!(
            z.wait_for(0, PROMPT, Duration::from_secs(5)),
            "no prompt within 5 s; output so far: {:?}",
            z.screen_since_start()
        );
        z
    }

    /// Reads whatever the shell has printed (non-blocking) into `screen`.
    fn pump(&mut self) {
        let mut buf = [0u8; 4096];
        loop {
            match self.session.try_read(&mut buf) {
                Ok(0) => break,
                Ok(n) => self.screen.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }

    /// Polls until `needle` appears in the output at or after byte offset `from`.
    fn wait_for(&mut self, from: usize, needle: &str, within: Duration) -> bool {
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
        self.session.write_all(keys.as_bytes()).unwrap();
        self.session.flush().unwrap();
    }

    /// Types `line`, presses Enter, and waits for the next prompt (10 s at most).
    pub fn run(&mut self, line: &str) {
        self.pump();
        let mark = self.screen.len();
        self.send(&format!("{line}\r"));
        // The command's own newline comes first; the next prompt after it ends the command.
        let ok = self.wait_for(mark, "\r\n", Duration::from_secs(10)) && {
            let nl = mark
                + String::from_utf8_lossy(&self.screen[mark..])
                    .find("\r\n")
                    .unwrap()
                + 2;
            self.wait_for(nl, PROMPT, Duration::from_secs(10))
        };
        assert!(
            ok,
            "no prompt after {line:?}; output: {:?}",
            self.screen_since_start()
        );
    }

    /// Asks the shell for its line state through the dump widget.
    pub fn state(&mut self) -> LineState {
        let dump = self.home_path.join("dump");
        let _ = std::fs::remove_file(&dump);
        self.send(DUMP_KEY);
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            self.pump();
            if let Ok(text) = std::fs::read_to_string(&dump) {
                if text.ends_with(&format!("{US}END")) {
                    return parse_dump(&text);
                }
            }
            if Instant::now() >= end {
                panic!(
                    "no dump within 3 s; output: {:?}",
                    self.screen_since_start()
                );
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Polls `state()` until `pred` holds (3 s); panics with the last state otherwise.
    pub fn wait_state(&mut self, pred: impl Fn(&LineState) -> bool) -> LineState {
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            let s = self.state();
            if pred(&s) {
                return s;
            }
            if Instant::now() >= end {
                panic!("state never matched; last state: {s:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// All pty output so far, lossy UTF-8.
    pub fn screen_since_start(&self) -> String {
        String::from_utf8_lossy(&self.screen).into_owned()
    }

    /// Like `screen_since_start`, after reading anything still pending.
    pub fn screen_now(&mut self) -> String {
        self.pump();
        self.screen_since_start()
    }

    pub fn home(&self) -> &Path {
        &self.home_path
    }

    pub fn histfile(&self) -> PathBuf {
        self.home_path.join(".zsh_history")
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

fn parse_dump(text: &str) -> LineState {
    // buffer US cursor US postdisplay US highlights US pid US END; the buffer may itself hold
    // a US byte, so split from the right.
    let mut it = text.rsplitn(6, US);
    let _end = it.next();
    let pid = it.next().unwrap_or("");
    let hl = it.next().unwrap_or("");
    let post = it.next().unwrap_or("");
    let cursor = it.next().unwrap_or("0");
    let buffer = it.next().unwrap_or("");
    LineState {
        buffer: buffer.to_string(),
        cursor: cursor.parse().unwrap_or(0),
        postdisplay: post.to_string(),
        region_highlight: hl
            .split('\u{1e}')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        core_pid: pid.parse().ok(),
    }
}
