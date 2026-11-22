//! Pty harnesses that drive a real interactive zsh, bash or fish with an adapter loaded.
//! Everything lives under a temporary HOME; the shell gets an empty environment plus a few
//! variables, so no real rc file, history or data is ever read or written.
#![allow(dead_code)] // each test file uses only its own shell's harness.

pub mod bash;
pub mod fish;
pub mod pty;
pub mod zsh;

#[allow(unused_imports)]
pub use pty::write_script;
#[allow(unused_imports)]
pub use zsh::{zsh_at_least, zsh_path, LineState, ZshHarness, ZshOpts};

use std::path::{Path, PathBuf};

/// The repo's `target/<shell>/<ver>/bin/<shell>` copies, newest version first.
fn built_shells(shell: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target")
        .join(shell);
    let mut found: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let ver: Vec<u32> = name
                .split('.')
                .map(|p| p.parse().ok())
                .collect::<Option<_>>()?;
            let bin = e.path().join("bin").join(shell);
            bin.is_file()
                .then(|| (ver, std::fs::canonicalize(bin).unwrap()))
        })
        .collect();
    found.sort();
    found.into_iter().rev().map(|(_, p)| p).collect()
}

fn shell_under_test(var: &str, shell: &str, build: &str) -> String {
    if let Ok(p) = std::env::var(var) {
        return p;
    }
    match built_shells(shell).into_iter().next() {
        Some(p) => p.display().to_string(),
        None => panic!("no {shell} to test: set {var}, or build one with `{build}`"),
    }
}

/// `$FUXX_TEST_BASH`, else the newest bash `scripts/build-bash.sh` built into `target/bash/`.
pub fn bash_path() -> String {
    shell_under_test("FUXX_TEST_BASH", "bash", "scripts/build-bash.sh 5.3")
}

/// `$FUXX_TEST_FISH`, else the newest fish `scripts/build-fish.sh` built into `target/fish/`.
pub fn fish_path() -> String {
    shell_under_test("FUXX_TEST_FISH", "fish", "scripts/build-fish.sh 4.9.3")
}

/// `$FUXX_TEST_BLESH`, else the newest `ble.sh` `scripts/get-blesh.sh` unpacked into
/// `target/blesh/`.
pub fn blesh_path() -> String {
    if let Ok(p) = std::env::var("FUXX_TEST_BLESH") {
        return p;
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/blesh");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path().join("ble.sh"))
        .filter(|p| p.is_file())
        .collect();
    found.sort();
    match found.pop() {
        Some(p) => std::fs::canonicalize(p).unwrap().display().to_string(),
        None => panic!("no ble.sh to test: set FUXX_TEST_BLESH, or run `scripts/get-blesh.sh`"),
    }
}
