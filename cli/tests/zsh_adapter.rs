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
    // Set before and after the plugin loads, so the options are live whenever our code runs.
    let mut z = ZshHarness::start(ZshOpts {
        rc_before: "setopt ksharrays shwordsplit nounset warncreateglobal".into(),
        rc_after: "setopt ksharrays shwordsplit nounset warncreateglobal".into(),
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

#[test]
fn sourcing_the_plugin_leaves_user_options_alone() {
    let hostile = "setopt ksharrays shwordsplit nounset extendedglob autocd";
    let mut z = ZshHarness::start(ZshOpts {
        rc_before: hostile.into(),
        rc_after: hostile.into(),
        ..git_history()
    });
    z.run(
        "print -r -- ${options[ksharrays]}:${options[shwordsplit]}:${options[nounset]}:\
${options[extendedglob]}:${options[autocd]}:${options[localoptions]} >| $HOME/opts",
    );
    let opts = std::fs::read_to_string(z.home().join("opts")).unwrap();
    assert_eq!(opts.trim(), "on:on:on:on:on:off");
}

#[test]
fn sourcing_again_keeps_one_working_core() {
    let mut z = ZshHarness::start(git_history());
    z.send("git p");
    let before = z.wait_state(|s| s.postdisplay == "ush origin main");
    z.send("\x15");
    z.wait_state(|s| s.buffer.is_empty());
    z.run("source ~/.zshrc");
    z.send("git p");
    let after = z.wait_state(|s| s.postdisplay == "ush origin main");
    assert_eq!(before.core_pid, after.core_pid);
    assert!(alive(after.core_pid.unwrap()));
    let screen = printable(&z.screen_now()).to_lowercase();
    for bad in ["nested", "error", "not found", "failed"] {
        assert!(!screen.contains(bad), "{bad:?} on screen: {screen:?}");
    }
}

#[test]
fn user_widget_calling_wrapped_widgets_works() {
    let mut z = ZshHarness::start(ZshOpts {
        history: vec!["git 2 foo", "git status", "git push origin main"],
        rc_after: "my-two() { zle backward-delete-char; zle self-insert }\n\
                   zle -N my-two\n\
                   bindkey '^X2' my-two"
            .into(),
        ..Default::default()
    });
    z.send("git p");
    z.wait_state(|s| s.postdisplay == "ush origin main");
    z.send("\x182"); // ^X 2: delete "p", insert "2"
    let s = z.wait_state(|s| s.buffer == "git 2" && s.postdisplay == " foo");
    assert_eq!(s.region_highlight.len(), 1, "{s:?}");
    let screen = z.screen_now().to_lowercase();
    assert!(
        !screen.contains("nested") && !screen.contains("error"),
        "{screen:?}"
    );
}

#[test]
fn without_memo_our_highlight_entries_do_not_pile_up() {
    // Forces the path zsh < 5.9 takes: our entry is found by its exact string only.
    let mut z = ZshHarness::start(ZshOpts {
        rc_after: "_FUXX_AS_MEMO=0".into(),
        ..git_history()
    });
    z.send("git p");
    let s = z.wait_state(|s| s.postdisplay == "ush origin main");
    assert_eq!(s.region_highlight, vec!["5 20 fg=8".to_string()]);
    z.send("u");
    let s = z.wait_state(|s| s.buffer == "git pu" && s.postdisplay == "sh origin main");
    assert_eq!(s.region_highlight, vec!["6 20 fg=8".to_string()]);
    z.send("x");
    let s = z.wait_state(|s| s.buffer == "git pux" && s.postdisplay.is_empty());
    assert!(s.region_highlight.is_empty(), "{s:?}");
}

#[test]
fn programs_the_user_runs_do_not_inherit_adapter_fds() {
    // Version gate: zsh below 5.4 has no working close-on-exec for a moved descriptor (`sysopen -o
    // cloexec -u` loses the flag; verified on 5.3.1, fixed upstream 2017-05-23), and plain
    // `exec {fd}` fds are inherited there too, so no plugin code can satisfy this on 5.3.x.
    if !zsh_at_least("5.4") {
        return;
    }
    let mut z = ZshHarness::start(git_history());
    z.run("print -r -- $_FUXX_AS_WFD $_FUXX_AS_RFD $_FUXX_AS_KEEPFD >| $HOME/ours; /bin/ls /dev/fd >| $HOME/fds");
    let ours = std::fs::read_to_string(z.home().join("ours")).unwrap();
    let fds: Vec<String> = std::fs::read_to_string(z.home().join("fds"))
        .unwrap()
        .split_whitespace()
        .map(String::from)
        .collect();
    let ours: Vec<&str> = ours.split_whitespace().collect();
    assert!(!ours.is_empty(), "no adapter fds recorded");
    for n in ours {
        assert!(!fds.iter().any(|f| f == n), "fd {n} inherited: {fds:?}");
    }
}

#[test]
fn a_failed_start_ends_off_stops_the_core_and_leaves_nothing() {
    // Fail the open of our write end after the core was started: the cleanup must still run.
    let mut z = ZshHarness::start(ZshOpts {
        core_script: Some(r#"echo $$ > "$HOME/core.pid"; exec cat > /dev/null"#),
        rc_before: "mkdir -p $HOME/tmp; export TMPDIR=$HOME/tmp".into(),
        rc_after: "functions -c _fuxx_as_open _t_open\n\
                   _fuxx_as_open() { [[ $1 == w ]] && return 1; _t_open \"$@\" }"
            .into(),
        ..Default::default()
    });
    z.run("print -r -- $_FUXX_AS_STATE >| $HOME/st");
    z.run("echo again");
    let st = std::fs::read_to_string(z.home().join("st")).unwrap();
    assert_eq!(st.trim(), "off");
    let left: Vec<_> = std::fs::read_dir(z.home().join("tmp")).unwrap().collect();
    assert!(left.is_empty(), "temp folders left: {left:?}");
    // The core may have been stopped before its script wrote a pid; if it did, it must be gone.
    if let Ok(text) = std::fs::read_to_string(z.home().join("core.pid")) {
        let pid: u32 = text.trim().parse().unwrap();
        assert!(
            poll(Duration::from_secs(1), || !alive(pid)),
            "core {pid} left running"
        );
    }
    let screen = printable(&z.screen_now()).to_lowercase();
    assert!(
        !screen.contains("fuxx-completes") && !screen.contains("error"),
        "{screen:?}"
    );
}

#[test]
fn ghost_text_keeps_its_style_while_a_widget_redraws() {
    if !zsh_at_least("5.9") {
        return; // below 5.9 the entry is deliberately taken off while a widget runs
    }
    let mut z = ZshHarness::start(ZshOpts {
        rc_after:
            "my-dump() { zle -R; print -r -- \"${(pj:\\x1e:)region_highlight}\" >| $HOME/inner }\n\
                   zle -N my-dump\n\
                   bindkey '^X3' my-dump"
                .into(),
        ..git_history()
    });
    z.send("git p");
    z.wait_state(|s| s.postdisplay == "ush origin main");
    z.send("\x183");
    let inner = z.home().join("inner");
    assert!(poll(Duration::from_secs(3), || inner.exists()));
    let seen = std::fs::read_to_string(&inner).unwrap();
    assert!(
        seen.contains(MEMO),
        "our entry was gone inside the widget: {seen:?}"
    );
}

#[test]
fn a_request_left_in_flight_by_a_slow_command_does_not_kill_the_core() {
    // The stub answers 2.5 s after each request; the command outlasts the 2 s hang limit.
    let mut z = ZshHarness::start(ZshOpts {
        core_script: Some(
            r#"US=$(printf '\037'); echo $$ > "$HOME/core.pid"
IFS= read -r hello; printf 'ready%s1\n' "$US"
while IFS=$US read -r kind id rest; do sleep 2.5; printf 'suggestion%s%s%s\n' "$US" "$id" "$US"; done"#,
        ),
        ..Default::default()
    });
    let pidfile = z.home().join("core.pid");
    assert!(poll(Duration::from_secs(3), || pidfile.exists()));
    let pid: u32 = std::fs::read_to_string(&pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    z.send("e");
    z.wait_state(|s| s.buffer == "e");
    z.send("\x15"); // the request for "e" stays in flight (the stub is still sleeping)
    z.run("sleep 2.3"); // Enter with that request in flight; the command outlasts 2 s
    z.send("a"); // first key at the next prompt
    z.wait_state(|s| s.buffer == "a");
    std::thread::sleep(Duration::from_millis(500));
    assert!(alive(pid), "the healthy core was killed");
    let s = z.state();
    assert_eq!(s.core_pid, Some(pid));
    let screen = printable(&z.screen_now()).to_lowercase();
    assert!(!screen.contains("error"), "{screen:?}");
}

#[test]
fn without_zsh_system_pipes_still_work() {
    // zmodload refuses zsh/system, as on a zsh built without it: plain `exec {fd}` is used.
    let mut z = ZshHarness::start(ZshOpts {
        rc_before: "zmodload() { [[ $1 == zsh/system ]] && return 1; builtin zmodload \"$@\" }"
            .into(),
        ..git_history()
    });
    z.send("git p");
    z.wait_state(|s| s.postdisplay == "ush origin main");
    z.send("\x15");
    z.run("print -r -- $_FUXX_AS_CLOEXEC >| $HOME/cx");
    let cx = std::fs::read_to_string(z.home().join("cx")).unwrap();
    assert_eq!(cx.trim(), "0");
}

// ---- Task 7: accept, partial accept, clear, End and ⌥→, vi mode ----

/// Types "git p" on the git history and waits for the ghost text.
fn with_ghost(opts: ZshOpts) -> ZshHarness {
    let mut z = ZshHarness::start(opts);
    z.send("git p");
    z.wait_state(|s| s.postdisplay == "ush origin main");
    z
}

/// `bindkey -v` goes in `rc_after` (the harness runs `bindkey -e` after `rc_before`); the test
/// widget's key is bound in both vi keymaps too, so `state()` works in either.
fn vi_opts() -> ZshOpts {
    ZshOpts {
        rc_after: "bindkey -v\n\
                   bindkey -M viins '^[[9999~' _fuxx-test-dump\n\
                   bindkey -M vicmd '^[[9999~' _fuxx-test-dump"
            .into(),
        ..git_history()
    }
}

/// Esc needs `KEYTIMEOUT` (10 ms in the harness) to pass before the next key is a command.
fn esc(z: &mut ZshHarness) {
    z.send("\x1b");
    std::thread::sleep(Duration::from_millis(150));
}

fn assert_fully_accepted(s: &common::LineState) {
    assert_eq!(s.buffer, "git push origin main", "{s:?}");
    assert_eq!(s.postdisplay, "", "{s:?}");
    assert!(s.region_highlight.is_empty(), "{s:?}");
}

#[test]
fn right_arrow_accepts_whole_suggestion() {
    let mut z = with_ghost(git_history());
    z.send("\x1b[C");
    let s = z.wait_state(|s| s.buffer == "git push origin main");
    assert_fully_accepted(&s);
    assert_eq!(s.cursor, 20);
}

#[test]
fn ctrl_e_accepts_whole_suggestion() {
    let mut z = with_ghost(git_history());
    z.send("\x05");
    let s = z.wait_state(|s| s.buffer == "git push origin main");
    assert_fully_accepted(&s);
    assert_eq!(s.cursor, 20);
}

#[test]
fn esc_f_accepts_one_word() {
    // zshzle: `forward-word` moves "to the beginning of the next word", so the accepted part keeps
    // the trailing space (parity with zsh-autosuggestions). Ruling (controller, Task 7).
    let mut z = with_ghost(git_history());
    z.send("\x1bf");
    let s = z.wait_state(|s| s.buffer == "git push ");
    assert_eq!(s.postdisplay, "origin main");
    assert_eq!(s.cursor, 9);
    let want = if zsh_at_least("5.9") {
        format!("9 20 fg=8 {MEMO}")
    } else {
        "9 20 fg=8".to_string()
    };
    assert_eq!(s.region_highlight, vec![want]);
}

#[test]
fn forward_char_mid_buffer_only_moves() {
    let mut z = with_ghost(git_history());
    z.send("\x02"); // ⌃B: cursor to 4
    z.send("\x1b[C");
    let s = z.wait_state(|s| s.cursor == 5);
    assert_eq!(s.buffer, "git p");
    assert_eq!(s.postdisplay, "ush origin main");
}

#[test]
fn end_key_accepts_when_free() {
    for seq in ["\x1b[F", "\x1b[4~"] {
        let mut z = with_ghost(git_history());
        z.send(seq);
        let s = z.wait_state(|s| s.buffer == "git push origin main");
        assert_fully_accepted(&s);
    }
}

#[test]
fn alt_right_accepts_a_word_when_free() {
    // Same `forward-word` semantics as above (zshzle). Ruling (controller, Task 7).
    let mut z = with_ghost(git_history());
    z.send("\x1b[1;3C");
    let s = z.wait_state(|s| s.buffer == "git push ");
    assert_eq!(s.postdisplay, "origin main");
}

#[test]
fn user_binding_of_end_is_kept() {
    let mut z = with_ghost(ZshOpts {
        rc_before: "bindkey '^[[F' beginning-of-line".into(),
        ..git_history()
    });
    z.send("\x1b[F");
    let s = z.wait_state(|s| s.cursor == 0);
    assert_eq!(s.buffer, "git p");
    assert_eq!(s.postdisplay, "ush origin main");
}

#[test]
fn user_binding_of_alt_right_is_kept() {
    let mut z = with_ghost(ZshOpts {
        rc_before: "bindkey '^[[1;3C' beginning-of-line".into(),
        ..git_history()
    });
    z.send("\x1b[1;3C");
    let s = z.wait_state(|s| s.cursor == 0);
    assert_eq!(s.buffer, "git p");
    assert_eq!(s.postdisplay, "ush origin main");
}

#[test]
fn accept_line_clears_ghost_text() {
    let mut z = with_ghost(git_history());
    let mark = z.screen_now().len();
    z.send("\r");
    std::thread::sleep(Duration::from_millis(300));
    // Only what the shell drew after Enter: the line is redrawn without the ghost text.
    let after = z.screen_now()[mark..].to_string();
    assert!(
        !printable(&after).contains("ush origin"),
        "ghost text stayed on the line: {after:?}"
    );
}

#[test]
fn up_arrow_clears_ghost_text() {
    let mut z = with_ghost(git_history());
    z.send("\x1b[A");
    let s = z.wait_state(|s| s.postdisplay.is_empty());
    assert!(s.region_highlight.is_empty(), "{s:?}");
}

#[test]
fn vi_end_of_line_accepts() {
    let mut z = with_ghost(vi_opts());
    esc(&mut z);
    z.send("$");
    let s = z.wait_state(|s| s.buffer == "git push origin main");
    assert_fully_accepted(&s);
    assert_eq!(s.cursor, 19, "vicmd cursor sits on the last character");
}

#[test]
fn vi_add_eol_accepts() {
    let mut z = with_ghost(vi_opts());
    esc(&mut z);
    z.send("A");
    let s = z.wait_state(|s| s.buffer == "git push origin main");
    assert_fully_accepted(&s);
    assert_eq!(s.cursor, 20);
}

#[test]
fn vi_forward_word_accepts_part() {
    let mut z = with_ghost(vi_opts());
    esc(&mut z);
    z.send("w");
    let s = z.wait_state(|s| s.buffer == "git push o");
    assert_eq!(s.postdisplay, "rigin main");
}

#[test]
fn tab_ctrl_r_ctrl_t_are_not_bound_by_us() {
    const DUMP: &str = "for k in '^I' '^R' '^T' '^[[A'; do bindkey -- $k; done";
    let mut z = ZshHarness::start(ZshOpts {
        // Before the plugin's first precmd: the bindings as zsh and the rc leave them.
        rc_after: format!("{DUMP} > \"$HOME/bind-before\""),
        ..git_history()
    });
    // After the first prompt the plugin has started and made its bindings.
    z.run(&format!("{DUMP} > \"$HOME/bind-after\""));
    let before = std::fs::read_to_string(z.home().join("bind-before")).unwrap();
    let after = std::fs::read_to_string(z.home().join("bind-after")).unwrap();
    assert!(before.contains("\"^I\""), "empty dump: {before:?}");
    assert_eq!(before, after);
}

// ---- Task 8: command reports (§5.8) and the history re-read ----
// Reports are one prompt late, so a test that expects a command to be suggested runs one more
// command (`run("true")`) after it.

/// Records the wire. It keeps a copy of its stdout (the answer pipe) open on fd 3: with stdout
/// redirected away the shell would see end-of-file on the answer pipe, take the core for dead
/// and stop, which is right for a dead core but not what these tests are about.
const RECORDER: &str = r#"exec 3>&1; exec cat > "$HOME/wire.log""#;

/// Types `typed`, waits for the ghost text to be `want`, lets any late report land, and checks
/// it is still `want` (a hidden line wrongly reported would replace it).
fn assert_suggestion_stays(z: &mut ZshHarness, typed: &str, want: &str) {
    z.send(typed);
    z.wait_state(|s| s.postdisplay == want);
    std::thread::sleep(Duration::from_millis(300));
    z.send("\x7f");
    z.send(&typed[typed.len() - 1..]);
    std::thread::sleep(Duration::from_millis(100));
    let s = z.wait_state(|s| s.postdisplay == want);
    assert_eq!(s.postdisplay, want, "{s:?}");
}

#[test]
fn command_run_in_session_is_suggested() {
    let mut z = ZshHarness::start(ZshOpts::default());
    z.run("echo one");
    z.run("true");
    z.send("echo o");
    z.wait_state(|s| s.postdisplay == "ne");
}

#[test]
fn report_waits_for_the_next_command() {
    let mut z = ZshHarness::start(ZshOpts {
        core_script: Some(RECORDER),
        ..Default::default()
    });
    let home = z.home().to_path_buf();
    z.run("echo one");
    std::thread::sleep(Duration::from_secs(1));
    assert!(
        !read_wire(&home).lines().any(|l| l.starts_with("ran")),
        "{:?}",
        read_wire(&home)
    );
    z.run("true");
    assert!(
        poll(Duration::from_secs(3), || read_wire(&home).lines().any(
            |l| l.starts_with("ran") && l.ends_with(&format!("{US}echo one"))
        )),
        "wire: {:?}",
        read_wire(&home)
    );
    assert!(!read_wire(&home)
        .lines()
        .any(|l| l.starts_with("ran") && l.ends_with(&format!("{US}true"))));
}

#[test]
fn space_led_command_is_not_suggested() {
    let mut z = ZshHarness::start(ZshOpts {
        rc_before: "setopt histignorespace".into(),
        ..Default::default()
    });
    z.run("echo shown");
    z.run(" echo secret");
    z.run("true");
    assert_suggestion_stays(&mut z, "echo s", "hown");
}

#[test]
fn history_ignore_match_is_not_suggested() {
    let mut z = ZshHarness::start(ZshOpts {
        rc_before: "HISTORY_IGNORE='echo secret*'".into(),
        ..Default::default()
    });
    z.run("echo shown");
    z.run("echo secret1");
    z.run("true");
    assert_suggestion_stays(&mut z, "echo s", "hown");
}

#[test]
fn line_rejected_by_another_zshaddhistory_hook_is_not_suggested() {
    let mut z = ZshHarness::start(ZshOpts {
        rc_after: "_t_rej() { [[ $1 != 'echo secret'* ]] }; add-zsh-hook zshaddhistory _t_rej"
            .into(),
        ..Default::default()
    });
    z.run("echo shown");
    z.run("echo secret1");
    z.run("true");
    assert_suggestion_stays(&mut z, "echo s", "hown");
}

#[test]
fn alias_with_leading_space_is_not_suggested() {
    let mut z = ZshHarness::start(ZshOpts {
        rc_before: "setopt histignorespace".into(),
        rc_after: "alias sp=' echo hidden'".into(),
        ..Default::default()
    });
    z.run("sx=1");
    z.run("sp");
    z.run("true");
    assert_suggestion_stays(&mut z, "s", "x=1");
}

#[test]
fn repeated_hidden_line_is_not_suggested() {
    let mut z = ZshHarness::start(ZshOpts {
        rc_before: "setopt histignorespace".into(),
        ..Default::default()
    });
    z.run("echo shown");
    z.run(" echo secret");
    z.run(" echo secret");
    z.run("true");
    assert_suggestion_stays(&mut z, "echo s", "hown");
}

#[test]
fn reports_exit_status_and_start_cwd() {
    let mut z = ZshHarness::start(ZshOpts {
        core_script: Some(RECORDER),
        ..Default::default()
    });
    let home = z.home().to_path_buf();
    z.run("mkdir sub; cd sub");
    z.run("false");
    z.run("true");
    let want = format!("ran{US}{}/sub{US}1{US}", home.display());
    assert!(
        poll(Duration::from_secs(3), || read_wire(&home).lines().any(
            |l| {
                l.strip_prefix(&want).is_some_and(|rest| {
                    rest.strip_suffix(&format!("{US}false"))
                        .is_some_and(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()))
                })
            }
        )),
        "wire: {:?}",
        read_wire(&home)
    );
}

#[test]
fn our_hook_never_rejects_a_line() {
    let mut z = ZshHarness::start(ZshOpts::default());
    z.run("echo kept");
    z.run("fc -ln -1 >| $HOME/fc");
    let got = std::fs::read_to_string(z.home().join("fc")).unwrap();
    assert_eq!(got.trim(), "echo kept");
}

#[test]
fn history_file_appended_by_another_shell_is_seen() {
    let mut z = ZshHarness::start(git_history());
    z.send("git p");
    z.wait_state(|s| s.postdisplay == "ush origin main");
    z.send("\x15");
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(z.histfile())
        .unwrap();
    std::io::Write::write_all(&mut f, b"git commit -m wip\n").unwrap();
    drop(f);
    z.send("git c");
    z.wait_state(|s| s.postdisplay == "ommit -m wip");
}

#[test]
fn history_file_rewritten_without_a_line_drops_it() {
    let mut z = ZshHarness::start(ZshOpts {
        history: vec!["rm -rf build", "ls"],
        ..Default::default()
    });
    z.send("rm");
    z.wait_state(|s| s.postdisplay == " -rf build");
    z.send("\x15");
    let tmp = z.home().join("hist.tmp");
    std::fs::write(&tmp, "ls\n").unwrap();
    std::fs::rename(&tmp, z.histfile()).unwrap();
    z.send("rm");
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        // Each retype is a fresh request.
        let s = z.state();
        if s.postdisplay.is_empty() && s.buffer == "rm" {
            break;
        }
        assert!(Instant::now() < end, "still suggested: {s:?}");
        std::thread::sleep(Duration::from_millis(100));
        z.send("\x7f");
        z.send("m");
    }
}

#[test]
fn no_histfile_session_commands_still_suggested() {
    let mut z = ZshHarness::start(ZshOpts {
        rc_after: "unset HISTFILE".into(),
        ..Default::default()
    });
    z.run("echo hi");
    z.run("true");
    z.send("echo h");
    z.wait_state(|s| s.postdisplay == "i");
}

#[test]
fn history_ignore_is_matched_under_kshglob() {
    let mut z = ZshHarness::start(ZshOpts {
        rc_before: "setopt kshglob; HISTORY_IGNORE='@(echo secret)*'".into(),
        ..Default::default()
    });
    z.run("echo shown");
    z.run("echo secret1");
    z.run("true");
    assert_suggestion_stays(&mut z, "echo s", "hown");
}

#[test]
fn plugin_loads_without_parse_error_under_shglob() {
    // Ghost text only appears if `_fuxx_as_precmd` was defined and started the core. (zsh's own
    // add-zle-hook-widget dispatcher prints "No such widget" on every prompt under SH_GLOB, which
    // is zsh's, not ours; so this test types no command and looks only for the parse error.)
    let mut z = ZshHarness::start(ZshOpts {
        rc_before: "setopt shglob".into(),
        ..git_history()
    });
    z.send("git p");
    z.wait_state(|s| s.postdisplay == "ush origin main");
    let screen = printable(&z.screen_now()).to_lowercase();
    assert!(!screen.contains("parse error"), "screen: {screen:?}");
}

#[test]
fn invalid_history_ignore_prints_nothing_and_reports_nothing() {
    let mut z = ZshHarness::start(ZshOpts {
        rc_before: "HISTORY_IGNORE='('".into(),
        core_script: Some(RECORDER),
        ..Default::default()
    });
    let home = z.home().to_path_buf();
    z.run("echo one");
    z.run("true");
    z.run("true");
    std::thread::sleep(Duration::from_millis(300));
    let screen = printable(&z.screen_now()).to_lowercase();
    assert!(!screen.contains("bad pattern"), "screen: {screen:?}");
    assert!(
        !read_wire(&home).lines().any(|l| l.starts_with("ran")),
        "wire: {:?}",
        read_wire(&home)
    );
}
