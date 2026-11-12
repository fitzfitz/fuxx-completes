//! The zsh adapter in a real interactive zsh on a pty (temporary HOME, empty environment).

mod common;

use common::{zsh_at_least, ZshHarness, ZshOpts};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const US: char = '\u{1f}';
const MEMO: &str = "memo=fuxx-autosuggestions";

fn git_history() -> ZshOpts {
    ZshOpts {
        history: vec!["git status", "git push origin main"],
        ..Default::default()
    }
}

/// Polls `f` for up to `within`; true as soon as it returns true.
fn poll(within: Duration, mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + within;
    loop {
        if f() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn read_wire(home: &std::path::Path) -> String {
    std::fs::read_to_string(home.join("wire.log")).unwrap_or_default()
}

/// `kill -0` succeeds while the process exists.
fn alive(pid: u32) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Removes terminal escape sequences, `\r` and the `%`-line marker zsh prints before a prompt.
fn printable(screen: &str) -> String {
    let mut out = String::new();
    let mut chars = screen.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('@'..='~').contains(&n) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    for n in chars.by_ref() {
                        if n == '\u{7}' {
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
}

#[test]
fn ghost_text_appears_for_a_history_prefix() {
    let mut z = ZshHarness::start(git_history());
    z.send("git p");
    let s = z.wait_state(|s| s.postdisplay == "ush origin main");
    assert_eq!(s.buffer, "git p");
}

#[test]
fn ghost_text_is_dimmed_and_tagged() {
    let mut z = ZshHarness::start(git_history());
    z.send("git p");
    let s = z.wait_state(|s| s.postdisplay == "ush origin main");
    let want = if zsh_at_least("5.9") {
        format!("5 20 fg=8 {MEMO}")
    } else {
        "5 20 fg=8".to_string()
    };
    assert_eq!(s.region_highlight, vec![want]);
}

#[test]
fn highlight_style_is_configurable() {
    let mut z = ZshHarness::start(ZshOpts {
        rc_before: "FUXX_AUTOSUGGESTIONS_HIGHLIGHT_STYLE=fg=red".into(),
        ..git_history()
    });
    z.send("git p");
    let s = z.wait_state(|s| s.postdisplay == "ush origin main");
    assert_eq!(s.region_highlight.len(), 1);
    assert!(s.region_highlight[0].contains("fg=red"), "{s:?}");
}

#[test]
fn typing_a_non_match_clears_ghost_text() {
    let mut z = ZshHarness::start(git_history());
    z.send("git p");
    z.wait_state(|s| s.postdisplay == "ush origin main");
    z.send("x");
    let s = z.wait_state(|s| s.buffer == "git px" && s.postdisplay.is_empty());
    assert!(s.region_highlight.is_empty(), "left over highlight: {s:?}");
}

#[test]
fn foreign_region_highlight_entry_survives() {
    let mut z = ZshHarness::start(ZshOpts {
        rc_after: "_t_init() { region_highlight+=( '0 1 bold' ) }\n\
                   add-zle-hook-widget line-init _t_init"
            .into(),
        ..git_history()
    });
    z.send("git p");
    let s = z.wait_state(|s| s.postdisplay == "ush origin main");
    // zsh itself moves an entry when text is inserted at or before it (plain zsh turns "0 1 bold"
    // into "5 6 bold" after typing five characters), so look for the entry by its style.
    let bold = |s: &common::LineState| {
        s.region_highlight
            .iter()
            .filter(|e| e.ends_with(" bold"))
            .count()
    };
    assert_eq!(bold(&s), 1, "{s:?}");
    z.send("x");
    let s = z.wait_state(|s| s.buffer == "git px" && s.postdisplay.is_empty());
    assert_eq!(bold(&s), 1, "{s:?}");
    assert_eq!(
        s.region_highlight.len(),
        1,
        "only the foreign entry is left: {s:?}"
    );
}

#[test]
fn prints_nothing_at_startup() {
    let z = ZshHarness::start(git_history());
    let shown = printable(&z.screen_since_start());
    let shown: String = shown
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '%')
        .collect();
    assert_eq!(shown, "READY>", "raw: {:?}", z.screen_since_start());
}

#[test]
fn sends_hello_then_one_suggest_in_flight() {
    let mut z = ZshHarness::start(ZshOpts {
        core_script: Some(r#"exec cat > "$HOME/wire.log""#),
        ..Default::default()
    });
    z.send("abc");
    let home = z.home().to_path_buf();
    let hello = format!(
        "hello{US}1{US}zsh{US}{}{US}zsh{US}{}{US}autosuggestions\n",
        zsh_version(),
        z.histfile().display()
    );
    assert!(
        poll(Duration::from_secs(3), || {
            read_wire(&home).contains(&format!("suggest{US}1{US}async{US}1{US}"))
        }),
        "wire: {:?}",
        read_wire(&home)
    );
    // Give the other two keys time to (wrongly) produce more requests.
    std::thread::sleep(Duration::from_millis(300));
    let wire = read_wire(&home);
    assert_eq!(
        wire,
        format!(
            "{hello}suggest{US}1{US}async{US}1{US}{}{US}a\n",
            home.display()
        )
    );
}

fn zsh_version() -> String {
    let out = Command::new(common::zsh_path())
        .args(["-fc", "print -rn -- $ZSH_VERSION"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn missing_binary_stays_silent() {
    let mut z = ZshHarness::start(ZshOpts {
        core_bin: Some(PathBuf::from("/nonexistent")),
        ..git_history()
    });
    z.run("echo hi");
    let screen = z.screen_now();
    assert!(screen.contains("\r\nhi\r\n"), "{screen:?}");
    assert!(!screen.contains("fuxx"), "{screen:?}");
}

#[test]
fn core_killed_mid_session_goes_quiet() {
    let mut z = ZshHarness::start(git_history());
    z.send("git p");
    let s = z.wait_state(|s| s.postdisplay == "ush origin main");
    let pid = s.core_pid.expect("core pid");
    z.send("\x15"); // ^U
    z.wait_state(|s| s.buffer.is_empty());
    Command::new("/bin/kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap();
    z.run("echo hi");
    let screen = z.screen_now();
    assert!(screen.contains("\r\nhi\r\n"), "{screen:?}");
    assert!(!screen.contains("fuxx"), "{screen:?}");
    let lower = screen.to_lowercase();
    for bad in ["error", "not found", "bad", "failed", "no such"] {
        assert!(!lower.contains(bad), "{bad:?} on screen: {screen:?}");
    }
    z.send("git p");
    std::thread::sleep(Duration::from_millis(300));
    let s = z.state();
    assert_eq!(s.buffer, "git p");
    assert_eq!(s.postdisplay, "");
}

#[test]
fn slow_core_never_blocks_typing() {
    let mut z = ZshHarness::start(ZshOpts {
        core_script: Some(r#"echo $$ > "$HOME/core.pid"; exec cat > /dev/null"#),
        ..Default::default()
    });
    let t = Instant::now();
    z.run("echo typed");
    assert!(
        t.elapsed() < Duration::from_secs(1),
        "took {:?}",
        t.elapsed()
    );
    assert!(z.screen_now().contains("\r\ntyped\r\n"));
}

#[test]
fn hung_core_is_stopped_after_two_seconds() {
    let mut z = ZshHarness::start(ZshOpts {
        core_script: Some(r#"echo $$ > "$HOME/core.pid"; exec cat > /dev/null"#),
        ..Default::default()
    });
    let pidfile = z.home().join("core.pid");
    assert!(poll(Duration::from_secs(3), || pidfile.exists()));
    z.send("e");
    z.wait_state(|s| s.buffer == "e");
    std::thread::sleep(Duration::from_millis(2500));
    z.send("c");
    let pid: u32 = std::fs::read_to_string(&pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(
        poll(Duration::from_secs(1), || !alive(pid)),
        "hung core {pid} still alive"
    );
}

#[test]
fn adapter_survives_hostile_user_options() {
    let mut z = ZshHarness::start(ZshOpts {
        rc_before: "setopt ksharrays shwordsplit nounset warncreateglobal".into(),
        ..git_history()
    });
    z.send("git p");
    z.wait_state(|s| s.postdisplay == "ush origin main");
    z.run("echo ok");
    let screen = printable(&z.screen_now()).to_lowercase();
    for bad in [
        "error",
        "not found",
        "bad",
        "failed",
        "no such",
        "parameter",
    ] {
        assert!(!screen.contains(bad), "{bad:?} on screen: {screen:?}");
    }
}

#[test]
fn multiline_buffer_gets_suggestion() {
    let mut z = ZshHarness::start(ZshOpts {
        history: vec!["for i in 1 2\ndo\n  echo $i\ndone"],
        ..Default::default()
    });
    z.send("for i in 1 2");
    // The core reads history in the background after its first request; wait until it has
    // answered, so the newline's own request is not the one that races the load.
    z.wait_state(|s| s.postdisplay == "\ndo\n  echo $i\ndone");
    z.send("\x1b\r"); // self-insert-unmeta: a newline in the buffer
    let s = z.wait_state(|s| s.postdisplay == "do\n  echo $i\ndone");
    assert_eq!(s.buffer, "for i in 1 2\n");
}

#[test]
fn separator_byte_in_buffer_sends_nothing() {
    let mut z = ZshHarness::start(ZshOpts {
        core_script: Some(r#"exec cat > "$HOME/wire.log""#),
        ..Default::default()
    });
    z.send("\x16\x1fa"); // ^V then 0x1F, then "a"
    let home = z.home().to_path_buf();
    assert!(poll(Duration::from_secs(3), || read_wire(&home)
        .contains("hello")));
    z.wait_state(|s| s.buffer.ends_with('a') && s.buffer.len() == 2);
    std::thread::sleep(Duration::from_secs(1));
    let wire = read_wire(&home);
    assert!(wire.starts_with("hello"), "{wire:?}");
    assert!(!wire.lines().any(|l| l.starts_with("suggest")), "{wire:?}");
    z.send("\x15"); // ^U
    z.run("echo ok");
    assert!(z.screen_now().contains("\r\nok\r\n"));
}
