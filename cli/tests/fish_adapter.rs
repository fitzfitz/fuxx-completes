//! The fish adapter in a real interactive fish on a pty (temporary HOME, empty environment).
//! The fish under test is `$FUXX_TEST_FISH`, else the newest one `scripts/build-fish.sh` built.
//! The core is a recorder script that appends its arguments to `$HOME/wire.log`.

mod common;

use common::fish::{FishHarness, FishOpts};
use std::time::{Duration, Instant};

/// Logs each call's arguments as one line.
const RECORDER: &str = "echo \"$*\" >> \"$HOME/wire.log\"";
/// The same, a second late.
const SLOW_RECORDER: &str = "sleep 1; echo \"$*\" >> \"$HOME/wire.log\"";

fn wire(f: &FishHarness) -> String {
    std::fs::read_to_string(f.home().join("wire.log")).unwrap_or_default()
}

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

fn recorder() -> FishOpts {
    FishOpts {
        core_script: Some(RECORDER),
        ..Default::default()
    }
}

/// The `report` line expected for `command` with `status`, run in `cwd`.
fn report_line(f: &FishHarness, cwd: &str, status: i32, command: &str) -> String {
    format!(
        "report --cwd {cwd} --status {status} --histfile {} -- {command}",
        f.histfile().display()
    )
}

#[test]
fn harness_runs_fish_in_a_temp_home() {
    let mut f = FishHarness::start(FishOpts::default());
    let home = f.home().to_path_buf();
    f.run("echo \"$HOME\"");
    assert!(
        f.screen_now().contains(&home.display().to_string()),
        "HOME is not the temp folder: {:?}",
        f.screen_now()
    );
}

#[test]
fn commands_are_reported() {
    let mut f = FishHarness::start(recorder());
    let home = f.home().display().to_string();
    f.run("ls");
    f.run("false");
    let want = report_line(&f, &home, 1, "false");
    assert!(
        poll(Duration::from_secs(3), || wire(&f)
            .lines()
            .any(|l| l == want)),
        "want {want:?} in {:?}",
        wire(&f)
    );
    assert!(wire(&f)
        .lines()
        .any(|l| l == report_line(&f, &home, 0, "ls")));
}

#[test]
fn cwd_is_where_it_started() {
    let mut f = FishHarness::start(recorder());
    let home = f.home().display().to_string();
    f.run("mkdir sub");
    f.run("cd sub");
    f.run("echo here");
    assert!(
        poll(Duration::from_secs(3), || wire(&f).contains("echo here")),
        "{:?}",
        wire(&f)
    );
    let w = wire(&f);
    assert!(
        w.lines().any(|l| l == report_line(&f, &home, 0, "cd sub")),
        "{w:?}"
    );
    assert!(
        w.lines()
            .any(|l| l == report_line(&f, &format!("{home}/sub"), 0, "echo here")),
        "{w:?}"
    );
}

#[test]
fn filters_and_off_cases() {
    // A leading space, private mode, fish_history set empty: nothing.
    let mut f = FishHarness::start(recorder());
    f.run(" echo spaced");
    f.run("set -g fish_private_mode 1");
    f.run("echo private");
    f.run("set -e fish_private_mode");
    f.run("set -g fish_history ''");
    f.run("echo nohistory");
    f.run("set -e fish_history");
    f.run("echo last");
    assert!(
        poll(Duration::from_secs(3), || wire(&f).contains("echo last")),
        "{:?}",
        wire(&f)
    );
    let w = wire(&f);
    for hidden in ["spaced", "echo private", "echo nohistory"] {
        assert!(!w.contains(hidden), "{hidden} was reported: {w:?}");
    }

    // Warp: nothing at all.
    let mut f = FishHarness::start(FishOpts {
        core_script: Some(RECORDER),
        env: vec![("TERM_PROGRAM", "WarpTerminal".into())],
        ..Default::default()
    });
    f.run("echo warp");
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(wire(&f), "");

    // fish 4.0+: a user's fish_should_add_to_history decides alone.
    let f0 = FishHarness::start(FishOpts::default());
    if !f0.at_least(4) {
        return;
    }
    let mut f = FishHarness::start(FishOpts {
        core_script: Some(RECORDER),
        config_before: "function fish_should_add_to_history; string match -q '*secret*' -- $argv[1]; and return 1; return 0; end".into(),
        ..Default::default()
    });
    let home = f.home().display().to_string();
    f.run("echo secret");
    f.run(" echo spaced");
    let want = report_line(&f, &home, 0, " echo spaced");
    assert!(
        poll(Duration::from_secs(3), || wire(&f)
            .lines()
            .any(|l| l == want)),
        "want {want:?} in {:?}",
        wire(&f)
    );
    assert!(!wire(&f).contains("echo secret"), "{:?}", wire(&f));
}

#[test]
fn reports_never_hold_the_prompt() {
    let mut f = FishHarness::start(FishOpts {
        core_script: Some(SLOW_RECORDER),
        ..Default::default()
    });
    for i in 0..5 {
        let t = Instant::now();
        f.run(&format!("echo n{i}"));
        assert!(
            t.elapsed() < Duration::from_millis(300),
            "prompt {i} took {:?}",
            t.elapsed()
        );
    }
    std::thread::sleep(Duration::from_millis(1500));
    f.run("echo done");
    let screen = f.screen_now();
    assert!(!screen.contains("has ended"), "{screen:?}");
    assert!(!screen.contains("Job "), "{screen:?}");
    assert!(
        poll(Duration::from_secs(3), || wire(&f)
            .lines()
            .filter(|l| l.starts_with("report"))
            .count()
            >= 5),
        "{:?}",
        wire(&f)
    );
}

#[test]
fn never_starts_serve() {
    let mut f = FishHarness::start(recorder());
    f.run("echo one");
    f.run("echo two");
    assert!(
        poll(Duration::from_secs(3), || wire(&f).contains("echo two")),
        "{:?}",
        wire(&f)
    );
    assert!(
        !wire(&f).lines().any(|l| l.starts_with("serve")),
        "{:?}",
        wire(&f)
    );
}

#[test]
fn doctor_through_the_function() {
    let mut f = FishHarness::start(FishOpts::default());
    f.run("fuxx-completes doctor");
    let screen = f.screen_now();
    let version = f.version().to_string();
    assert!(
        screen.contains(&format!("shell        fish {version}")),
        "{screen}"
    );
    assert!(
        screen.contains("state        off (fish has its own autosuggestions)"),
        "{screen}"
    );
    assert!(screen.contains("reporting    on"), "{screen}");
}
