//! The bash harness: `$FUXX_TEST_BASH --noprofile --rcfile $HOME/.bashrc -i` with the adapter
//! loaded, an empty `INPUTRC` and a generated history, on the shared pty core.

use super::pty::{clean_command, core_path, temp_home, Pty};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

pub const PROMPT: &str = "READY> ";
const US: char = '\u{1f}';
const DUMP_KEY: &str = "\x1b[9999~";
/// Under ble.sh the dump is bound with `ble-bind -x` to ⌃X d.
const BLE_DUMP_KEY: &str = "\x18d";

#[derive(Default)]
pub struct BashOpts {
    /// History entries; with `history_times` each gets a `#<time>` line before it.
    pub history: Vec<&'static str>,
    pub history_times: bool,
    pub rc_before: String,
    pub rc_after: String,
    /// A stub core: this `/bin/sh` script is written to `$HOME/core.sh` and used as the core
    /// (wins over `core_bin`). The recorder is `exec cat > "$HOME/wire.log"`.
    pub core_script: Option<&'static str>,
    pub core_bin: Option<PathBuf>,
    /// Extra environment for the shell.
    pub env: Vec<(&'static str, String)>,
    /// Load ble.sh (`common::blesh_path()`) around the rc file.
    pub blesh: bool,
    /// The bash binary; default `common::bash_path()`.
    pub bash: Option<String>,
    /// The terminal's size (columns, rows); default 120×40.
    pub size: Option<(u16, u16)>,
    /// Run with the temp HOME before the shell starts (to seed data files).
    pub setup: Option<fn(&Path)>,
}

#[derive(Default, Debug, Clone)]
pub struct BashState {
    /// `READLINE_LINE`
    pub buffer: String,
    /// `READLINE_POINT` in characters (bash before 5.0 counts bytes; converted here).
    pub point: usize,
    /// `_FUXX_AS_STATE`
    pub state: String,
    /// `_FUXX_AS_WHY`
    pub why: String,
    /// `_FUXX_AS_DIR`: the pipes' folder, until the adapter read the core's `ready`.
    pub pipes: String,
}

pub struct BashHarness {
    pty: Pty,
    major: u32,
    blesh: bool,
}

/// bash's major version (`BASH_VERSINFO[0]`).
pub fn bash_major(bash: &str) -> u32 {
    Command::new(bash)
        .args(["--norc", "--noprofile", "-c", "echo ${BASH_VERSINFO[0]}"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0)
}

impl BashHarness {
    pub fn start(opts: BashOpts) -> Self {
        let (home, home_path) = temp_home();
        let bash = opts.bash.clone().unwrap_or_else(super::bash_path);

        let mut hist = String::new();
        for (i, entry) in opts.history.iter().enumerate() {
            if opts.history_times {
                hist.push_str(&format!("#{}\n", 1_700_000_000 + i));
            }
            hist.push_str(entry);
            hist.push('\n');
        }
        std::fs::write(home_path.join(".bash_history"), hist).unwrap();
        std::fs::write(home_path.join(".inputrc"), "").unwrap();
        // ble.sh wants its cache folder to exist and USER set.
        for d in [".cache", ".local/state"] {
            std::fs::create_dir_all(home_path.join(d)).unwrap();
        }
        // The adapter's pipe folder goes under the temp HOME, so a core that never says `ready`
        // (a stub) leaves nothing behind in /tmp.
        std::fs::create_dir_all(home_path.join("tmp")).unwrap();

        let plugin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../adapters/bash/fuxx-autosuggestions.bash");
        let (ble_load, ble_attach) = if opts.blesh {
            (
                format!("source '{}' --noattach\n", super::blesh_path()),
                "[[ ${BLE_VERSION-} ]] && { ble-bind -x 'C-x d' _fuxx_test_dump; ble-attach; }\n"
                    .to_string(),
            )
        } else {
            (String::new(), String::new())
        };
        let rc = format!(
            "{ble_load}{before}\n\
             HISTFILE=$HOME/.bash_history; HISTSIZE=1000; HISTFILESIZE=1000; PS1='{PROMPT}'\n\
             [ -f '{plugin}' ] && source '{plugin}'\n\
             _fuxx_test_dump() {{\n\
               printf '%s\\037%s\\037%s\\037%s\\037%s\\037END' \"$READLINE_LINE\" \"$READLINE_POINT\" \"${{_FUXX_AS_STATE-}}\" \"${{_FUXX_AS_WHY-}}\" \"${{_FUXX_AS_DIR-}}\" > \"$HOME/dump\"\n\
             }}\n\
             # bind -x can't take a sequence longer than two keys before bash 4.3 (CHANGES 4.3-alpha pp):\n\
             # there the dump key is a macro for a two-key sequence bound with bind -x.\n\
             if (( BASH_VERSINFO[0] > 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] >= 3) )); then\n\
               bind -x '\"\\e[9999~\": _fuxx_test_dump'\n\
             else\n\
               bind -x '\"\\C-x\\C-_\": _fuxx_test_dump'; bind '\"\\e[9999~\": \"\\C-x\\C-_\"'\n\
             fi\n\
             {after}\n{ble_attach}",
            before = opts.rc_before,
            plugin = plugin.display(),
            after = opts.rc_after,
        );
        std::fs::write(home_path.join(".bashrc"), rc).unwrap();

        if let Some(setup) = opts.setup {
            setup(&home_path);
        }
        if opts.blesh {
            // No character-width probing (it asks the terminal where the cursor is) and no paste
            // detection (the harness types a line and Enter in one write).
            std::fs::write(
                home_path.join(".blerc"),
                "bleopt char_width_mode=east\nbleopt accept_line_threshold=-1\n",
            )
            .unwrap();
        }
        let core = core_path(&home_path, opts.core_script, opts.core_bin);
        // The pty library turns the terminal's echo off before the shell starts; a real terminal
        // has it on, and readline before bash 4.4 echoes typed keys only when it is on, so turn
        // it back on first.
        let mut cmd = clean_command("/bin/sh", &home_path);
        cmd.args(["-c", "stty echo 2>/dev/null; exec \"$@\"", "sh", &bash])
            .args(["--noprofile", "--rcfile"])
            .arg(home_path.join(".bashrc"))
            .arg("-i")
            .env("INPUTRC", home_path.join(".inputrc"))
            .env("HISTFILE", home_path.join(".bash_history"))
            .env("BASH_SILENCE_DEPRECATION_WARNING", "1")
            .env("TMPDIR", home_path.join("tmp"))
            .env("USER", "tester")
            .env("FUXX_COMPLETES_BIN", core);
        if opts.blesh {
            // ble.sh's caches (keymaps, terminfo) are shared by the tests, so it builds them once.
            let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/blesh/test-cache");
            std::fs::create_dir_all(&cache).unwrap();
            cmd.env("XDG_CACHE_HOME", cache);
        }
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        let major = bash_major(&bash);
        // (ble.sh asks the terminal for device attributes and the cursor position but doesn't
        // wait for the answers; left unanswered, as script(1) leaves them.)
        let pty = Pty::spawn(cmd, opts.size, (home, home_path));
        let mut b = BashHarness {
            pty,
            major,
            blesh: opts.blesh,
        };
        assert!(
            b.pty.wait_for(0, PROMPT, Duration::from_secs(20)),
            "no prompt within 10 s; output so far: {:?}",
            b.screen_since_start()
        );
        b
    }

    /// bash's major version.
    pub fn major(&self) -> u32 {
        self.major
    }

    pub fn wait_screen(&mut self, needle: &str) -> bool {
        self.pty.wait_screen(needle)
    }

    pub fn send(&mut self, keys: &str) {
        self.pty.send(keys)
    }

    pub fn send_bytes(&mut self, bytes: &[u8]) {
        self.pty.send_bytes(bytes)
    }

    /// Types `line`, presses Enter, and waits for the next prompt (10 s at most).
    pub fn run(&mut self, line: &str) {
        self.pty.run(line, PROMPT)
    }

    /// Asks the shell for its line state through the `bind -x` dump key.
    pub fn state(&mut self) -> BashState {
        let dump = self.home().join("dump");
        let _ = std::fs::remove_file(&dump);
        self.send(if self.blesh { BLE_DUMP_KEY } else { DUMP_KEY });
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            self.pty.pump();
            if let Ok(text) = std::fs::read_to_string(&dump) {
                if text.ends_with(&format!("{US}END")) {
                    return self.parse_dump(&text);
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
    pub fn wait_state(&mut self, pred: impl Fn(&BashState) -> bool) -> BashState {
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

    fn parse_dump(&self, text: &str) -> BashState {
        // line US point US state US why US pipes US END; the line may hold a US byte, so split
        // from the right.
        let mut it = text.rsplitn(6, US);
        let _end = it.next();
        let pipes = it.next().unwrap_or("").to_string();
        let why = it.next().unwrap_or("");
        let state = it.next().unwrap_or("");
        let point: usize = it.next().unwrap_or("0").parse().unwrap_or(0);
        let buffer = it.next().unwrap_or("").to_string();
        // ble.sh's READLINE_POINT counts characters in every bash.
        let point = if self.major < 5 && !self.blesh {
            let bytes = buffer.as_bytes();
            String::from_utf8_lossy(&bytes[..point.min(bytes.len())])
                .chars()
                .count()
        } else {
            point
        };
        BashState {
            buffer,
            point,
            state: state.to_string(),
            why: why.to_string(),
            pipes,
        }
    }

    pub fn screen_since_start(&self) -> String {
        self.pty.screen_since_start()
    }

    pub fn screen_now(&mut self) -> String {
        self.pty.screen_now()
    }

    pub fn bytes(&self) -> &[u8] {
        self.pty.bytes()
    }

    pub fn home(&self) -> &Path {
        self.pty.home()
    }

    pub fn histfile(&self) -> PathBuf {
        self.home().join(".bash_history")
    }
}
