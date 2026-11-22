//! `forget` and `doctor` as real processes, with HOME and XDG_DATA_HOME in a temp folder.

use fuxx_completes_core::context::secret::Secret;
use fuxx_completes_core::context::store::append;
use fuxx_completes_core::context::{record::Record, Key};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_fuxx-completes");
const USAGE: &str = "usage: fuxx-completes serve | forget | doctor [--report] | why [--format zsh|bash] [--cwd DIR] [--histfile FILE] [--] LINE | report --cwd DIR --status N [--time SECS] --histfile FILE [--] COMMAND";

fn run(home: &Path, env: &[(&str, &Path)], args: &[&str]) -> Output {
    let mut c = Command::new(BIN);
    c.env_clear()
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("xdg"))
        .args(args);
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().unwrap()
}

/// A secret and one context file under `root`.
fn seed(root: &Path) {
    Secret::load_or_create(root).unwrap();
    let r = Record {
        key: Key(1),
        folder: PathBuf::from("/p"),
        status: 0,
        time: 1,
    };
    append(root, "0123456789abcdef0123456789abcdef", &[r]).unwrap();
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn forget_removes_and_reports() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("xdg/fuxx-completes");
    seed(&root);
    let out = run(home.path(), &[], &["forget"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stdout(&out),
        format!("Removed the context files in {}.\n", root.display())
    );
    assert!(!root.exists());
}

#[test]
fn forget_twice_says_nothing_to_remove() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("xdg/fuxx-completes");
    seed(&root);
    run(home.path(), &[], &["forget"]);
    let out = run(home.path(), &[], &["forget"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stdout(&out),
        format!("Nothing to remove in {}.\n", root.display())
    );
}

#[test]
fn forget_with_plugin_data_keeps_the_root() {
    let home = tempfile::tempdir().unwrap();
    let pdata = home.path().join("pdata");
    seed(&pdata);
    let out = run(home.path(), &[("FUXX_PLUGIN_DATA", &pdata)], &["forget"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(pdata.exists());
    assert!(!pdata.join("key").exists());
    assert!(!pdata.join("context").exists());
}

#[test]
fn forget_reports_an_error_and_exits_1() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("xdg/fuxx-completes");
    seed(&root);
    // A folder we can't change: the removal fails.
    let ctx = root.join("context");
    std::fs::set_permissions(&ctx, std::os::unix::fs::PermissionsExt::from_mode(0o500)).unwrap();
    let out = run(home.path(), &[], &["forget"]);
    std::fs::set_permissions(&ctx, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("fuxx-completes: could not remove"));
}

#[test]
fn unknown_subcommand_prints_usage_exit_2() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        &["nope"][..],
        &[][..],
        &["forget", "x"][..],
        &["doctor", "--shell-facts"][..],
        &["why"][..],
        &["why", "--cwd"][..],
        &["report"][..],
        &[
            "report",
            "--cwd",
            "/x",
            "--status",
            "x",
            "--histfile",
            "/h",
            "--",
            "ls",
        ][..],
        &["report", "--status", "0", "--histfile", "/h", "--", "ls"][..],
        &["report", "--cwd", "/x", "--status", "0", "--", "ls"][..],
        &[
            "report",
            "--cwd",
            "/x",
            "--status",
            "0",
            "--histfile",
            "/h",
            "--",
        ][..],
        &[
            "report",
            "--cwd",
            "/x",
            "--status",
            "0",
            "--time",
            "soon",
            "--histfile",
            "/h",
            "--",
            "ls",
        ][..],
    ] {
        let out = run(home.path(), &[], args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(String::from_utf8_lossy(&out.stderr), format!("{USAGE}\n"));
    }
}

// ---- M2: report (fish) ----

/// The records in the own file of history file `hist` under data root `root`.
fn own_records(root: &Path, hist: &str) -> Vec<Record> {
    let secret = Secret::load(root).unwrap().expect("a secret");
    let id = fuxx_completes_core::context::store::own_file_id(&secret, Path::new(hist));
    let bytes = std::fs::read(root.join("context").join(id)).unwrap_or_default();
    bytes
        .split(|&b| b == b'\n')
        .filter_map(fuxx_completes_core::context::record::decode)
        .collect()
}

/// Every byte of every file under `dir`, for "the text never lands on disk" checks.
fn all_bytes(dir: &Path) -> Vec<u8> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(all_bytes(&p));
        } else {
            out.extend(std::fs::read(&p).unwrap_or_default());
        }
    }
    out
}

#[test]
fn report_appends_one_record() {
    let home = tempfile::tempdir().unwrap();
    let args = [
        "report",
        "--cwd",
        "/x",
        "--status",
        "1",
        "--time",
        "5",
        "--histfile",
        "/h",
        "--",
        "make test",
    ];
    let out = run(home.path(), &[], &args);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty() && out.stderr.is_empty(), "{out:?}");
    let root = home.path().join("xdg/fuxx-completes");
    let recs = own_records(&root, "/h");
    assert_eq!(recs.len(), 1, "{recs:?}");
    let secret = Secret::load(&root).unwrap().unwrap();
    assert_eq!(recs[0].key, secret.key_of("make test"));
    assert_eq!((recs[0].status, recs[0].time), (1, 5));
    assert_eq!(recs[0].folder, PathBuf::from("/x"));
    let bytes = all_bytes(&root);
    assert!(
        !bytes.windows(9).any(|w| w == b"make test"),
        "the command text is on disk"
    );
}

#[test]
fn report_without_time_uses_now() {
    let home = tempfile::tempdir().unwrap();
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let out = run(
        home.path(),
        &[],
        &[
            "report",
            "--cwd",
            "/x",
            "--status",
            "0",
            "--histfile",
            "/h",
            "--",
            "ls",
        ],
    );
    assert_eq!(out.status.code(), Some(0));
    let recs = own_records(&home.path().join("xdg/fuxx-completes"), "/h");
    assert_eq!(recs.len(), 1);
    assert!(
        recs[0].time >= before && recs[0].time <= before + 5,
        "{recs:?}"
    );
}

#[test]
fn report_is_silent_without_data() {
    let out = Command::new(BIN)
        .env_clear()
        .args([
            "report",
            "--cwd",
            "/x",
            "--status",
            "0",
            "--histfile",
            "/h",
            "--",
            "ls",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty() && out.stderr.is_empty(), "{out:?}");
}

#[test]
fn report_usage_errors_exit_2() {
    let home = tempfile::tempdir().unwrap();
    let out = run(home.path(), &[], &["report", "--cwd", "/x"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!home.path().join("xdg").exists(), "nothing written");
}

#[test]
fn report_trims_past_the_cap() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("xdg/fuxx-completes");
    let secret = Secret::load_or_create(&root).unwrap();
    let id = fuxx_completes_core::context::store::own_file_id(&secret, Path::new("/h"));
    let one = Record {
        key: secret.key_of("old"),
        folder: PathBuf::from("/p"),
        status: 0,
        time: 1,
    };
    let len = fuxx_completes_core::context::record::encode(&one).len() as u64;
    let n = fuxx_completes_core::context::store::CAP / len + 50;
    let recs: Vec<Record> = (0..n).map(|_| one.clone()).collect();
    append(&root, &id, &recs).unwrap();
    let out = run(
        home.path(),
        &[],
        &[
            "report",
            "--cwd",
            "/x",
            "--status",
            "0",
            "--time",
            "9",
            "--histfile",
            "/h",
            "--",
            "new",
        ],
    );
    assert_eq!(out.status.code(), Some(0));
    let size = std::fs::metadata(root.join("context").join(&id))
        .unwrap()
        .len();
    assert!(
        size <= fuxx_completes_core::context::store::TRIM_TO,
        "{size}"
    );
    let recs = own_records(&root, "/h");
    assert!(recs
        .iter()
        .any(|r| r.key == secret.key_of("new") && r.time == 9));
}

#[test]
fn doctor_runs_outside_a_shell() {
    let home = tempfile::tempdir().unwrap();
    let out = run(home.path(), &[], &["doctor"]);
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(text.starts_with("fuxx-completes "), "{text}");
    assert!(text.contains("run this inside zsh"), "{text}");
    let out = run(
        home.path(),
        &[],
        &[
            "doctor",
            "--shell-facts",
            "shell=zsh\nversion=5.9\nstate=on\n",
        ],
    );
    assert!(stdout(&out).lines().any(|l| l == "state        on"));
}

// ---- M1d: why and doctor --report ----

/// A temp HOME with a history file, a data folder holding a secret, and records
/// `(command, folder, status)`.
fn why_setup(
    history: &[&str],
    recs: &[(&str, &Path, i32)],
) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(home.path()).unwrap();
    let hist = base.join(".zsh_history");
    std::fs::write(
        &hist,
        history.iter().map(|h| format!("{h}\n")).collect::<String>(),
    )
    .unwrap();
    let root = base.join("xdg/fuxx-completes");
    if !recs.is_empty() {
        let secret = Secret::load_or_create(&root).unwrap();
        let records: Vec<Record> = recs
            .iter()
            .enumerate()
            .map(|(i, (c, f, st))| Record {
                key: secret.key_of(c),
                folder: f.to_path_buf(),
                status: *st,
                time: 1_000_000 + i as i64,
            })
            .collect();
        let id = fuxx_completes_core::context::store::own_file_id(&secret, &hist);
        append(&root, &id, &records).unwrap();
    }
    (home, base, hist)
}

fn why(base: &Path, cwd: &Path, hist: Option<&Path>, line: &str) -> String {
    let mut args = vec!["why".to_string(), "--cwd".into(), cwd.display().to_string()];
    if let Some(h) = hist {
        args.push("--histfile".into());
        args.push(h.display().to_string());
    }
    args.push("--".into());
    args.push(line.into());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run(base, &[], &args);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout(&out)
}

#[test]
fn why_reads_a_bash_history() {
    let home = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(home.path()).unwrap();
    let hist = base.join(".bash_history");
    std::fs::write(&hist, "#1700000000\ngit status\n#1700000001\nls\n").unwrap();
    let out = run(
        &base,
        &[],
        &[
            "why",
            "--format",
            "bash",
            "--cwd",
            base.to_str().unwrap(),
            "--histfile",
            hist.to_str().unwrap(),
            "--",
            "git status",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = stdout(&out);
    assert!(
        text.lines().any(|l| l == "history    in your history"),
        "{text}"
    );
}

#[test]
fn why_explains_a_line_that_ran_here() {
    let dir = tempfile::tempdir().unwrap();
    let p = std::fs::canonicalize(dir.path()).unwrap();
    let (_h, base, hist) = why_setup(
        &["make test", "make build"],
        &[("make test", &p, 0), ("make test", &p, 0)],
    );
    let text = why(&base, &p, Some(&hist), "make test");
    assert!(
        text.lines()
            .any(|l| l == format!("folder     {}", p.display())),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("here       ok 2 · failed 0")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l == "step       this folder's history"),
        "{text}"
    );
    assert!(
        text.lines().any(|l| l == "history    in your history"),
        "{text}"
    );
}

#[test]
fn why_flags_secret_and_dangerous_lines() {
    let dir = tempfile::tempdir().unwrap();
    let p = std::fs::canonicalize(dir.path()).unwrap();
    let (_h, base, hist) = why_setup(&["TOKEN=abc make", "rm -rf build"], &[]);
    let text = why(&base, &p, Some(&hist), "TOKEN=abc make");
    assert!(
        text.contains("never shown: looks like it holds a secret"),
        "{text}"
    );
    let text = why(&base, &p, Some(&hist), "rm -rf build");
    assert!(
        text.contains("guarded: shown once you type \"rm -\""),
        "{text}"
    );
}

#[test]
fn why_names_the_project_and_file() {
    let dir = tempfile::tempdir().unwrap();
    let p = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(p.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
    std::fs::write(p.join("pnpm-lock.yaml"), "").unwrap();
    let (_h, base, hist) = why_setup(&[], &[]);
    let text = why(&base, &p, Some(&hist), "pnpm run dev");
    assert!(
        text.lines()
            .any(|l| l.starts_with("project    pnpm (package.json)")),
        "{text}"
    );
    assert!(
        text.lines().any(|l| l == "step       a project command"),
        "{text}"
    );
}

/// Review Focus 5: no data, no histfile, a line not in history; nothing is written.
#[test]
fn why_writes_nothing_and_works_without_data() {
    let dir = tempfile::tempdir().unwrap();
    let p = std::fs::canonicalize(dir.path()).unwrap();
    let (_h, base, hist) = why_setup(&["ls"], &[("ls", &p, 0)]);
    let listing = |d: &Path| -> Vec<String> {
        let mut v: Vec<String> = walk(d);
        v.sort();
        v
    };
    let before = listing(&base);
    let text = why(&base, &p, Some(&hist), "foo bar");
    assert!(text.contains("not in your history"), "{text}");
    let text = why(&base, &p, None, "ls");
    assert!(text.lines().any(|l| l.starts_with("history    ")), "{text}");
    assert_eq!(listing(&base), before, "why wrote something");
    // No data folder at all (no HOME).
    let out = Command::new(BIN)
        .env_clear()
        .args(["why", "--cwd", &p.display().to_string(), "--", "ls"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).contains("not in your history"));
}

/// Every file under `d`, with its size.
fn walk(d: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            let len = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            out.push(format!("{} {len}", p.display()));
        }
    }
    out
}

#[test]
fn doctor_report_is_fenced_with_a_warning() {
    let home = tempfile::tempdir().unwrap();
    let out = run(
        home.path(),
        &[
            ("TERM", Path::new("xterm-256color")),
            ("TERM_PROGRAM", Path::new("fuxx")),
        ],
        &[
            "doctor",
            "--shell-facts",
            "shell=zsh\nversion=5.9\nstate=on\n",
            "--report",
        ],
    );
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "Check this before you post it: it shows your data folder's path."
    );
    assert_eq!(lines[1], "```text");
    assert_eq!(*lines.last().unwrap(), "```");
    assert!(
        lines.iter().any(|l| l.starts_with("os           ")),
        "{text}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("arch         ")),
        "{text}"
    );
    assert!(
        lines.contains(&"terminal     fuxx, xterm-256color"),
        "{text}"
    );
    assert!(lines.contains(&"shell        zsh 5.9"), "{text}");
    // Without --report: no fence.
    let out = run(home.path(), &[], &["doctor"]);
    assert!(!stdout(&out).contains("```"));
    let out = run(home.path(), &[], &["doctor", "--report"]);
    assert!(stdout(&out).contains("```text"));
}
