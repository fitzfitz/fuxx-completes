//! The fish harness: `$FUXX_TEST_FISH -i` with `XDG_CONFIG_HOME/fish/config.fish` loading the
//! adapter, a prompt of `READY> ` and a generated history, on the shared pty core.

use super::pty::{clean_command, core_path, temp_home, Pty};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub const PROMPT: &str = "READY> ";

#[derive(Default)]
pub struct FishOpts {
    pub history: Vec<&'static str>,
    /// fish code run before the adapter is sourced.
    pub config_before: String,
    /// A stub core: this `/bin/sh` script is written to `$HOME/core.sh` and used as the core.
    pub core_script: Option<&'static str>,
    /// Extra environment for the shell.
    pub env: Vec<(&'static str, String)>,
    /// The terminal's size (columns, rows); default 120×40.
    pub size: Option<(u16, u16)>,
}

pub struct FishHarness {
    pty: Pty,
    version: String,
}

/// fish's version (`$version`), e.g. `4.9.3`.
pub fn fish_version(fish: &str) -> String {
    Command::new(fish)
        .args(["--no-config", "-c", "echo $version"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

impl FishHarness {
    pub fn start(opts: FishOpts) -> Self {
        let (home, home_path) = temp_home();
        let fish = super::fish_path();

        let data = home_path.join(".local/share/fish");
        std::fs::create_dir_all(&data).unwrap();
        let mut hist = String::new();
        for (i, entry) in opts.history.iter().enumerate() {
            hist.push_str(&format!(
                "- cmd: {}\n  when: {}\n",
                entry.replace('\\', "\\\\").replace('\n', "\\n"),
                1_700_000_000 + i
            ));
        }
        std::fs::write(data.join("fish_history"), hist).unwrap();

        let conf = home_path.join(".config/fish");
        std::fs::create_dir_all(&conf).unwrap();
        let plugin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../adapters/fish/fuxx-autosuggestions.fish");
        let config = format!(
            "set -g fish_greeting ''\n\
             function fish_prompt; printf '{PROMPT}'; end\n\
             function fish_right_prompt; end\n\
             function fish_title; end\n\
             {before}\n\
             test -f '{plugin}'; and source '{plugin}'\n",
            before = opts.config_before,
            plugin = plugin.display(),
        );
        std::fs::write(conf.join("config.fish"), config).unwrap();

        let core = core_path(&home_path, opts.core_script, None);
        let mut cmd = clean_command(&fish, &home_path);
        cmd.arg("-i").env("FUXX_COMPLETES_BIN", core);
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        let version = fish_version(&fish);
        let mut pty = Pty::spawn(cmd, opts.size, (home, home_path));
        pty.answer_queries();
        let mut f = FishHarness { pty, version };
        assert!(
            f.pty.wait_for(0, PROMPT, Duration::from_secs(10)),
            "no prompt within 10 s; output so far: {:?}",
            f.screen_since_start()
        );
        f
    }

    /// fish's version, e.g. `4.9.3`.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// True when fish is at least `major`.
    pub fn at_least(&self, major: u32) -> bool {
        self.version
            .split('.')
            .next()
            .and_then(|m| m.parse::<u32>().ok())
            .is_some_and(|m| m >= major)
    }

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

    pub fn screen_since_start(&self) -> String {
        self.pty.screen_since_start()
    }

    pub fn screen_now(&mut self) -> String {
        self.pty.screen_now()
    }

    pub fn home(&self) -> &Path {
        self.pty.home()
    }

    pub fn histfile(&self) -> PathBuf {
        self.home().join(".local/share/fish/fish_history")
    }
}
