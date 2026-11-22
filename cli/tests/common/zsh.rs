//! The zsh harness: a real interactive zsh with the adapter loaded, on the shared pty core.

use super::pty::{clean_command, core_path, temp_home, Pty};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

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
    /// The terminal's size (columns, rows); default 120×40.
    pub size: Option<(u16, u16)>,
}

#[derive(Default, Debug, Clone)]
pub struct LineState {
    pub buffer: String,
    pub cursor: usize,
    pub postdisplay: String,
    pub region_highlight: Vec<String>,
    /// `$_FUXX_AS_PID`
    pub core_pid: Option<u32>,
    /// `$KEYMAP` (`main` in emacs mode, `viins` or `vicmd` in vi mode)
    pub keymap: String,
}

pub struct ZshHarness {
    pty: Pty,
}

impl ZshHarness {
    pub fn start(opts: ZshOpts) -> Self {
        let (home, home_path) = temp_home();

        let mut hist = String::new();
        for entry in &opts.history {
            hist.push_str(&entry.replace('\n', "\\\n"));
            hist.push('\n');
        }
        std::fs::write(home_path.join(".zsh_history"), hist).unwrap();

        let plugin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../adapters/zsh/fuxx-autosuggestions.plugin.zsh");
        let rc = format!(
            "FUXX_AUTOSUGGESTIONS_LIST=off\n{before}\n\
             HISTFILE=$HOME/.zsh_history; HISTSIZE=1000; SAVEHIST=1000; PS1='{PROMPT}'; bindkey -e; KEYTIMEOUT=1\n\
             source '{plugin}'\n\
             _fuxx-test-dump() {{\n\
               emulate -L zsh\n\
               print -rn -- \"$BUFFER\"$'\\x1f'\"$CURSOR\"$'\\x1f'\"$POSTDISPLAY\"$'\\x1f'\"${{(pj:\\x1e:)region_highlight}}\"$'\\x1f'\"${{_FUXX_AS_PID-}}\"$'\\x1f'\"$KEYMAP\"$'\\x1f'END >| \"$HOME/dump\"\n\
             }}\n\
             zle -N _fuxx-test-dump\n\
             bindkey '^[[9999~' _fuxx-test-dump\n\
             {after}\n",
            before = opts.rc_before,
            plugin = plugin.display(),
            after = opts.rc_after,
        );
        std::fs::write(home_path.join(".zshrc"), rc).unwrap();

        let core = core_path(&home_path, opts.core_script, opts.core_bin);
        let mut cmd = clean_command(&zsh_path(), &home_path);
        cmd.args(["-d", "-i"])
            .env("ZDOTDIR", &home_path)
            .env("FUXX_COMPLETES_BIN", core);
        let pty = Pty::spawn(cmd, opts.size, (home, home_path));
        let mut z = ZshHarness { pty };
        assert!(
            z.pty.wait_for(0, PROMPT, Duration::from_secs(5)),
            "no prompt within 5 s; output so far: {:?}",
            z.screen_since_start()
        );
        z
    }

    /// Waits (5 s at most) until `needle` shows in the output, without sending a key: a state
    /// dump is a key, and it would use up a read the shell made before a redraw (M1c review).
    pub fn wait_screen(&mut self, needle: &str) -> bool {
        self.pty.wait_screen(needle)
    }

    pub fn send(&mut self, keys: &str) {
        self.pty.send(keys)
    }

    /// Types `line`, presses Enter, and waits for the next prompt (10 s at most).
    pub fn run(&mut self, line: &str) {
        self.pty.run(line, PROMPT)
    }

    /// Asks the shell for its line state through the dump widget.
    pub fn state(&mut self) -> LineState {
        let dump = self.home().join("dump");
        let _ = std::fs::remove_file(&dump);
        self.send(DUMP_KEY);
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            self.pty.pump();
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
        self.pty.screen_since_start()
    }

    /// Like `screen_since_start`, after reading anything still pending.
    pub fn screen_now(&mut self) -> String {
        self.pty.screen_now()
    }

    /// The raw bytes the shell printed so far (read what is pending first with `screen_now`).
    pub fn bytes(&self) -> &[u8] {
        self.pty.bytes()
    }

    /// Writes raw bytes to the shell, as a terminal would.
    pub fn send_bytes(&mut self, bytes: &[u8]) {
        self.pty.send_bytes(bytes)
    }

    pub fn home(&self) -> &Path {
        self.pty.home()
    }

    pub fn histfile(&self) -> PathBuf {
        self.home().join(".zsh_history")
    }
}

fn parse_dump(text: &str) -> LineState {
    // buffer US cursor US postdisplay US highlights US pid US keymap US END; the buffer may
    // itself hold a US byte, so split from the right.
    let mut it = text.rsplitn(7, US);
    let _end = it.next();
    let keymap = it.next().unwrap_or("");
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
        keymap: keymap.to_string(),
    }
}
