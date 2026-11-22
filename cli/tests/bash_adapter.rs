//! The bash adapter in a real interactive bash on a pty (temporary HOME, empty environment).
//! The bash under test is `$FUXX_TEST_BASH`, else the newest one `scripts/build-bash.sh` built.

mod common;

use common::bash::{bash_major, BashHarness, BashOpts};
use std::time::{Duration, Instant};

const US: char = '\u{1f}';
const RS: char = '\u{1e}';
const RIGHT: &str = "\x1b[C";
const LEFT: &str = "\x1b[D";
const END: &str = "\x1b[F";

/// A recorder core: never answers, logs every line it is sent.
const RECORDER: &str = "exec cat > \"$HOME/wire.log\"";

/// Answers `ready`, then every `suggest` after 1 s with `tatus`.
const SLOW: &str = r#"us=$(printf '\037')
while IFS= read -r line; do
  case "$line" in
    hello*) printf 'ready\0371\n' ;;
    suggest*) id=$(printf '%s' "$line" | cut -d "$us" -f2)
              (sleep 1; printf 'suggestion\037%s\037tatus\n' "$id") & ;;
  esac
done"#;

/// The bash version under test as (major, minor).
fn version() -> (u32, u32) {
    let b = common::bash_path();
    let out = std::process::Command::new(&b)
        .args([
            "--norc",
            "--noprofile",
            "-c",
            "echo ${BASH_VERSINFO[0]} ${BASH_VERSINFO[1]}",
        ])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let mut it = text.split_whitespace().map(|n| n.parse().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0))
}

/// The key that accepts: → from bash 4.4 (before it every `bind -x` key redraws the prompt on a
/// new line, so → is left to readline there), End everywhere.
fn accept_key() -> &'static str {
    if version() >= (4, 4) {
        RIGHT
    } else {
        END
    }
}

fn git_history() -> BashOpts {
    BashOpts {
        history: vec!["git status", "git push origin main"],
        ..Default::default()
    }
}

fn wire(b: &BashHarness) -> String {
    std::fs::read_to_string(b.home().join("wire.log")).unwrap_or_default()
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

/// Waits until the adapter read the core's `ready` (a prompt reads it; a freshly built core can
/// take a few hundred ms to start on macOS), then a little for the history to load.
fn settle(b: &mut BashHarness) {
    b.wait_state(|s| s.state == "on");
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        b.run("");
        if b.state().pipes.is_empty() || Instant::now() >= end {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(50));
}

/// Removes terminal escape sequences and `\r`.
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
fn harness_runs_bash_in_a_temp_home() {
    let mut b = BashHarness::start(BashOpts::default());
    let home = b.home().to_path_buf();
    b.run("echo \"$HOME\"");
    assert!(
        b.screen_now().contains(&home.display().to_string()),
        "HOME is not the temp folder: {:?}",
        b.screen_now()
    );
    b.send("echo x");
    let s = b.wait_state(|s| s.buffer == "echo x");
    assert_eq!(s.point, 6);
}

#[test]
fn right_arrow_at_the_end_inserts() {
    if version() < (4, 4) {
        return; // → is readline's own there (accept_key); End covers it below.
    }
    let mut b = BashHarness::start(git_history());
    settle(&mut b);
    b.send("git s");
    b.wait_state(|s| s.buffer == "git s");
    b.send(RIGHT);
    let s = b.wait_state(|s| s.buffer == "git status");
    assert_eq!(s.point, 10);
}

#[test]
fn right_arrow_mid_line_moves() {
    let mut b = BashHarness::start(git_history());
    settle(&mut b);
    b.send("git s");
    b.send(LEFT);
    b.send(LEFT);
    b.wait_state(|s| s.point == 3);
    b.send(RIGHT);
    let s = b.wait_state(|s| s.point == 4);
    assert_eq!(s.buffer, "git s");
}

#[test]
fn end_inserts_at_the_end() {
    let mut b = BashHarness::start(git_history());
    settle(&mut b);
    b.send("git p");
    b.wait_state(|s| s.buffer == "git p");
    b.send(END);
    let s = b.wait_state(|s| s.buffer == "git push origin main");
    assert_eq!(s.point, 20);
    // Mid-line, End only moves to the end.
    b.send("\x01"); // ⌃A
    b.wait_state(|s| s.point == 0);
    b.send(END);
    let s = b.wait_state(|s| s.point == 20);
    assert_eq!(s.buffer, "git push origin main");
}

#[test]
fn multibyte_buffer_accepts_cleanly() {
    let mut b = BashHarness::start(BashOpts {
        history: vec!["echo héllo wörld"],
        ..Default::default()
    });
    settle(&mut b);
    b.send("echo hé");
    b.wait_state(|s| s.buffer == "echo hé");
    b.send(accept_key());
    let s = b.wait_state(|s| s.buffer == "echo héllo wörld");
    assert_eq!(s.point, 16);
    // Mid-line → moves over é as one character.
    if version() >= (4, 4) {
        b.send("\x01");
        for _ in 0..6 {
            b.send(RIGHT);
        }
        let s = b.wait_state(|s| s.point == 6);
        assert_eq!(s.buffer, "echo héllo wörld");
        b.send(RIGHT);
        b.wait_state(|s| s.point == 7);
    }

    // The wire's cursor is in code points.
    let mut b = BashHarness::start(BashOpts {
        core_script: Some(RECORDER),
        ..Default::default()
    });
    b.wait_state(|s| s.state == "on");
    b.send("echo hé");
    b.wait_state(|s| s.buffer == "echo hé");
    b.send(accept_key());
    assert!(
        poll(Duration::from_secs(3), || wire(&b).lines().any(
            |l| l.starts_with(&format!("suggest{US}1{US}sync{US}7{US}"))
        )),
        "{:?}",
        wire(&b)
    );
}

#[test]
fn slow_core_does_nothing_within_50ms() {
    let mut b = BashHarness::start(BashOpts {
        core_script: Some(SLOW),
        ..git_history()
    });
    b.wait_state(|s| s.state == "on");
    b.send("git s");
    b.wait_state(|s| s.buffer == "git s");
    let t = Instant::now();
    b.send(accept_key());
    let s = b.state();
    assert!(
        t.elapsed() < Duration::from_millis(200),
        "{:?}",
        t.elapsed()
    );
    assert_eq!(s.buffer, "git s");
    // The late answer for that request arrives; the next key gets nothing from it.
    std::thread::sleep(Duration::from_millis(1300));
    b.send(accept_key());
    let s = b.state();
    assert_eq!(s.buffer, "git s");
    assert_eq!(s.state, "on");
}

#[test]
fn reports_follow_history() {
    let mut b = BashHarness::start(BashOpts {
        core_script: Some(RECORDER),
        rc_before: "HISTCONTROL=ignorespace; HISTIGNORE=pwd".into(),
        ..Default::default()
    });
    let home = b.home().display().to_string();
    b.run("ls");
    b.run("false");
    b.run(" hidden_cmd");
    b.run("pwd");
    b.run("");
    b.run("echo end");
    assert!(
        poll(Duration::from_secs(3), || wire(&b).contains("echo end")),
        "{:?}",
        wire(&b)
    );
    let w = wire(&b);
    let ran: Vec<&str> = w.lines().filter(|l| l.starts_with("ran")).collect();
    assert_eq!(ran.len(), 3, "{w:?}");
    assert!(
        ran[0].starts_with(&format!("ran{US}{home}{US}0{US}"))
            && ran[0].ends_with(&format!("{US}ls")),
        "{w:?}"
    );
    assert!(
        ran[1].starts_with(&format!("ran{US}{home}{US}1{US}"))
            && ran[1].ends_with(&format!("{US}false")),
        "{w:?}"
    );
    let time: i64 = ran[1].split(US).nth(3).unwrap().parse().unwrap();
    assert!(time > 1_700_000_000, "{w:?}");
    assert!(
        w.lines().any(|l| l == format!("prev{US}false{US}1")),
        "{w:?}"
    );
    assert!(!w.contains("hidden_cmd"), "{w:?}");
    assert!(!w.contains(&format!("{US}pwd")), "{w:?}");
}

#[test]
fn cwd_is_where_the_command_started() {
    let mut b = BashHarness::start(BashOpts {
        core_script: Some(RECORDER),
        ..Default::default()
    });
    let home = b.home().display().to_string();
    b.run("mkdir sub");
    b.run("cd sub");
    b.run("echo here");
    assert!(
        poll(Duration::from_secs(3), || wire(&b).contains("echo here")),
        "{:?}",
        wire(&b)
    );
    let w = wire(&b);
    assert!(
        w.lines()
            .any(|l| l.starts_with(&format!("ran{US}{home}{US}0{US}")) && l.ends_with("cd sub")),
        "{w:?}"
    );
    assert!(
        w.lines()
            .any(|l| l.starts_with(&format!("ran{US}{home}/sub{US}0{US}"))
                && l.ends_with("echo here")),
        "{w:?}"
    );
}

#[test]
fn prompt_command_keeps_the_status() {
    let mut forms = vec!["PROMPT_COMMAND='echo $? >> \"$HOME/st\"'"];
    if version() >= (5, 1) {
        forms.push("PROMPT_COMMAND=('echo $? >> \"$HOME/st\"')");
    }
    for form in forms {
        let mut b = BashHarness::start(BashOpts {
            core_script: Some(RECORDER),
            rc_before: form.into(),
            ..Default::default()
        });
        b.run("false");
        b.run("true");
        assert!(
            poll(Duration::from_secs(3), || wire(&b)
                .contains(&format!("{US}true"))),
            "{form}: {:?}",
            wire(&b)
        );
        let st = std::fs::read_to_string(b.home().join("st")).unwrap();
        let lines: Vec<&str> = st.lines().collect();
        assert_eq!(&lines[lines.len() - 2..], ["1", "0"], "{form}: {st:?}");
        let w = wire(&b);
        assert!(
            w.lines().any(|l| l.starts_with("ran")
                && l.split(US).nth(2) == Some("1")
                && l.ends_with("false")),
            "{form}: {w:?}"
        );
    }
}

#[test]
fn fix_on_the_empty_line() {
    let mut b = BashHarness::start(BashOpts {
        history: vec!["ls"],
        ..Default::default()
    });
    settle(&mut b);
    // `ls` succeeds here (a record), then `lss` isn't found (127): the core's fix is `ls`.
    b.run("ls");
    b.run("lss");
    // `ls` lists the `tmp` folder of the temp HOME; it must not run again.
    let listed = b.screen_now().matches("tmp").count();
    b.send(accept_key());
    let s = b.wait_state(|s| s.buffer == "ls");
    assert_eq!(s.point, 2);
    assert_eq!(b.screen_now().matches("tmp").count(), listed, "nothing ran");
}

#[test]
fn cdpath_is_forwarded() {
    let mut b = BashHarness::start(BashOpts {
        core_script: Some(RECORDER),
        rc_before: "CDPATH=/a:/b".into(),
        ..Default::default()
    });
    b.wait_state(|s| s.state == "on");
    b.send("cd x");
    b.send(accept_key());
    assert!(
        poll(Duration::from_secs(3), || wire(&b)
            .contains(&format!("cd x{US}/a{RS}/b"))),
        "{:?}",
        wire(&b)
    );

    let mut b = BashHarness::start(BashOpts {
        core_script: Some(RECORDER),
        rc_before: "CDPATH=/a:/b; cd() { builtin cd \"$@\"; }".into(),
        ..Default::default()
    });
    b.wait_state(|s| s.state == "on");
    b.send("cd x");
    b.send(accept_key());
    assert!(
        poll(Duration::from_secs(3), || wire(&b)
            .lines()
            .any(|l| l.starts_with("suggest"))),
        "{:?}",
        wire(&b)
    );
    let w = wire(&b);
    let line = w.lines().find(|l| l.starts_with("suggest")).unwrap();
    assert_eq!(line.split(US).count(), 6, "{line:?}");
}

#[test]
fn stays_off_silently() {
    // macOS's bash 3.2: too old for plain readline.
    let mut b = BashHarness::start(BashOpts {
        core_script: Some(RECORDER),
        bash: Some("/bin/bash".into()),
        ..git_history()
    });
    assert_eq!(bash_major("/bin/bash"), 3);
    b.run("echo one");
    let s = b.state();
    assert_eq!((s.state.as_str(), s.why.as_str()), ("off", "bash-too-old"));
    assert!(!b.home().join("wire.log").exists(), "no core started");

    // Warp.
    let mut b = BashHarness::start(BashOpts {
        core_script: Some(RECORDER),
        env: vec![("TERM_PROGRAM", "WarpTerminal".into())],
        ..git_history()
    });
    b.run("echo one");
    let s = b.state();
    assert_eq!((s.state.as_str(), s.why.as_str()), ("off", "conflict:warp"));
    assert!(!b.home().join("wire.log").exists(), "no core started");

    // A user's own binding of the accept key is kept (→, or End before bash 4.4).
    let (key, seq) = if accept_key() == RIGHT {
        (RIGHT, "\\e[C")
    } else {
        (END, "\\e[F")
    };
    let mut b = BashHarness::start(BashOpts {
        rc_before: format!("bind '\"{seq}\": backward-char'"),
        ..git_history()
    });
    b.wait_state(|s| s.state == "on");
    b.send("git s");
    b.wait_state(|s| s.point == 5);
    b.send(key);
    let s = b.wait_state(|s| s.point == 4);
    assert_eq!(s.buffer, "git s");
    b.send("\x03"); // ⌃C: a fresh line
    b.run("fuxx-completes doctor");
    let screen = printable(&b.screen_now());
    assert!(
        screen.contains(&format!("keys         emacs {seq} kept (backward-char)")),
        "{screen}"
    );
}

#[test]
fn prints_nothing_at_startup_or_prompt() {
    let mut b = BashHarness::start(git_history());
    settle(&mut b);
    b.run("echo one");
    b.run("echo two");
    b.run("true");
    b.run("false");
    b.run("cd .");
    b.run("kill $_FUXX_AS_PID");
    b.run("echo three");
    b.run("echo four");
    let screen = printable(&b.screen_now());
    for line in screen.lines() {
        let ok = line.is_empty()
            || line.starts_with("READY> ")
            || ["one", "two", "three", "four"].contains(&line);
        assert!(ok, "unexpected line {line:?} in {screen}");
    }
    assert!(
        !screen.contains("[1]") && !screen.contains("Done"),
        "{screen}"
    );
}

#[test]
fn core_death_goes_quiet() {
    let mut b = BashHarness::start(git_history());
    settle(&mut b);
    b.run("kill $_FUXX_AS_PID");
    std::thread::sleep(Duration::from_millis(200));
    b.send("git s");
    b.wait_state(|s| s.buffer == "git s");
    b.send(accept_key());
    let s = b.wait_state(|s| s.state == "off");
    assert_eq!(s.buffer, "git s");
    assert_eq!(s.why, "exited");
    b.send("\x03");
    b.run("fuxx-completes doctor");
    let screen = printable(&b.screen_now());
    assert!(screen.contains("state        off (exited)"), "{screen}");
}

#[test]
fn a_core_that_stops_reading_never_blocks() {
    let mut b = BashHarness::start(BashOpts {
        core_script: Some(
            "IFS= read -r l; printf 'ready\\0371\\n'\n\
             while kill -0 \"$PPID\" 2>/dev/null; do sleep 0.2; done",
        ),
        ..Default::default()
    });
    for i in 0..300 {
        b.run(&format!(
            "echo {i} a-long-enough-command-line-to-fill-the-pipe-sooner-{i:0>40}"
        ));
    }
    let s = b.state();
    assert_eq!((s.state.as_str(), s.why.as_str()), ("off", "write-failed"));
}

#[test]
fn a_reading_core_keeps_up_with_many_reports() {
    let mut b = BashHarness::start(git_history());
    settle(&mut b);
    for i in 0..150 {
        b.run(&format!(
            "echo {i} a-long-enough-command-line-to-fill-the-pipe-sooner-{i:0>40}"
        ));
    }
    let s = b.state();
    assert_eq!(s.state, "on", "{s:?}");
    b.send("git s");
    b.send(accept_key());
    b.wait_state(|s| s.buffer == "git status");
}

#[test]
fn second_copy_stays_off() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../adapters/bash/fuxx-autosuggestions.bash");
    let mut b = BashHarness::start(BashOpts {
        rc_after: format!(
            "cp '{}' \"$HOME/copy.bash\"; source \"$HOME/copy.bash\"",
            src.display()
        ),
        ..git_history()
    });
    b.wait_state(|s| s.state == "on");
    b.run("fuxx-completes doctor");
    let screen = printable(&b.screen_now());
    let home = b.home().display().to_string();
    assert!(
        screen.contains(&format!(
            "copy         second copy stayed off: {home}/copy.bash"
        )),
        "{screen}"
    );
    b.send("git s");
    b.send(accept_key());
    b.wait_state(|s| s.buffer == "git status");
}

#[test]
fn doctor_and_why_through_the_function() {
    let mut b = BashHarness::start(git_history());
    settle(&mut b);
    b.run("fuxx-completes doctor");
    let screen = printable(&b.screen_now());
    let (maj, min) = version();
    assert!(
        screen.contains(&format!("shell        bash {maj}.{min}")),
        "{screen}"
    );
    assert!(screen.contains("state        on"), "{screen}");

    let mut b = BashHarness::start(BashOpts {
        core_script: Some(
            "echo \"$*\" >> \"$HOME/args.log\"\n\
             [ \"$1\" = serve ] && exec cat > /dev/null\nexit 0",
        ),
        ..git_history()
    });
    b.wait_state(|s| s.state == "on");
    b.run("fuxx-completes why git status");
    let home = b.home().display().to_string();
    let args = std::fs::read_to_string(b.home().join("args.log")).unwrap();
    let want =
        format!("why --format bash --cwd {home} --histfile {home}/.bash_history -- git status");
    assert!(args.lines().any(|l| l == want), "{args:?}");
}

// ---- ble.sh (Task 4; the user's decision U1 = B2) ----

fn blesh(opts: BashOpts) -> BashHarness {
    BashHarness::start(BashOpts {
        blesh: true,
        ..opts
    })
}

/// The plan's matrix pairs ble.sh with the newest bash (5.3); on the bash 4.0 built here ble.sh
/// showed no grey text of its own either (checked by hand, 2026-11-23), so the ble.sh tests run
/// only there.
fn blesh_bash_ok() -> bool {
    version() >= (5, 3)
}

/// ble.sh starts many processes; one ble.sh shell at a time keeps the other tests' timing sane.
static BLE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn ble_lock() -> std::sync::MutexGuard<'static, ()> {
    BLE.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn blesh_shows_our_ranking() {
    use fuxx_completes_core::context::record::Record;
    use fuxx_completes_core::context::secret::Secret;
    use fuxx_completes_core::context::store::{append, own_file_id};
    if !blesh_bash_ok() {
        return;
    }
    let _one = ble_lock();
    // A record: `git stash` succeeded in this folder, so it ranks above the newer `git status`.
    fn seed(home: &std::path::Path) {
        let root = home.join(".local/share/fuxx-completes");
        let secret = Secret::load_or_create(&root).unwrap();
        let rec = Record {
            key: secret.key_of("git stash"),
            folder: home.to_path_buf(),
            status: 0,
            time: 1_700_000_100,
        };
        append(
            &root,
            &own_file_id(&secret, &home.join(".bash_history")),
            &[rec],
        )
        .unwrap();
    }
    let mut b = blesh(BashOpts {
        history: vec!["git stash", "git status"],
        setup: Some(seed),
        ..Default::default()
    });
    settle(&mut b);
    b.send("git st");
    assert!(b.wait_screen("ash"), "no grey text: {:?}", b.screen_now());
    b.send(END);
    let s = b.wait_state(|s| s.buffer == "git stash");
    assert_eq!(s.point, 9);
}

#[test]
fn blesh_gives_way_to_typing() {
    if !blesh_bash_ok() {
        return;
    }
    let _one = ble_lock();
    let mut b = blesh(BashOpts {
        core_script: Some(SLOW),
        ..Default::default()
    });
    settle(&mut b);
    let t = Instant::now();
    b.send("abc");
    let s = b.wait_state(|s| s.buffer == "abc");
    assert!(
        t.elapsed() < Duration::from_millis(300),
        "{:?}",
        t.elapsed()
    );
    assert_eq!(s.state, "on");
    std::thread::sleep(Duration::from_millis(1300));
    assert!(!b.screen_now().contains("tatus"), "a late answer was shown");
}

#[test]
fn blesh_beside_atuin_stays_off() {
    if !blesh_bash_ok() {
        return;
    }
    let _one = ble_lock();
    let mut b = blesh(BashOpts {
        core_script: Some(RECORDER),
        rc_after: "function ble/complete/auto-complete/source:atuin-history { return 1; }\n\
                   ble/util/import/eval-after-load core-complete \
                   'ble/array#unshift _ble_complete_auto_source atuin-history'"
            .into(),
        ..git_history()
    });
    settle(&mut b);
    b.send("git s");
    // No key while ble.sh is idle (a state dump is a key, and ble.sh asks its sources only when
    // no key is waiting).
    std::thread::sleep(Duration::from_millis(1000));
    let s = b.state();
    assert_eq!(
        (s.state.as_str(), s.why.as_str()),
        ("off", "conflict:atuin")
    );
    assert!(
        !wire(&b).lines().any(|l| l.starts_with("suggest")),
        "{:?}",
        wire(&b)
    );
}

#[test]
fn blesh_reports_commands() {
    if !blesh_bash_ok() {
        return;
    }
    let _one = ble_lock();
    let mut b = blesh(BashOpts {
        core_script: Some(RECORDER),
        ..Default::default()
    });
    let home = b.home().display().to_string();
    settle(&mut b);
    b.run("false");
    b.run("echo two");
    assert!(
        poll(Duration::from_secs(3), || wire(&b)
            .contains(&format!("{US}echo two"))),
        "{:?}",
        wire(&b)
    );
    let w = wire(&b);
    assert!(
        w.lines()
            .any(|l| l.starts_with(&format!("ran{US}{home}{US}1{US}")) && l.ends_with("false")),
        "{w:?}"
    );
}

#[test]
fn blesh_on_bash_3_2_stays_off() {
    if !blesh_bash_ok() {
        return; // it doesn't use the bash under test; once per matrix is enough
    }
    let _one = ble_lock();
    let mut b = BashHarness::start(BashOpts {
        bash: Some("/bin/bash".into()),
        blesh: true,
        ..git_history()
    });
    b.run("echo one");
    // (ble.sh's `ble-bind -x` dump key doesn't work on bash 3.2; doctor tells the state.)
    b.run("fuxx-completes doctor");
    b.run("echo \"pid=[${_FUXX_AS_PID-}]\"");
    let screen = printable(&b.screen_now());
    assert!(
        screen.contains("state        off (bash-too-old)"),
        "{screen}"
    );
    assert!(screen.contains("pid=[]"), "no core started: {screen}");
}
