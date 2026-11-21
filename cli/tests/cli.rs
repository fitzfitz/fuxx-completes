//! `forget` and `doctor` as real processes, with HOME and XDG_DATA_HOME in a temp folder.

use fuxx_completes_core::context::secret::Secret;
use fuxx_completes_core::context::store::append;
use fuxx_completes_core::context::{record::Record, Key};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_fuxx-completes");

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
    ] {
        let out = run(home.path(), &[], args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            "usage: fuxx-completes serve | forget | doctor\n"
        );
    }
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
