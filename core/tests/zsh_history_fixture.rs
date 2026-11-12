//! The fixture was written by real zsh 5.9 (see the command in the M0 plan, Task 2), so it
//! has zsh's own metafied bytes and its own backslash-newline continuation.

use fuxx_completes_core::zsh_history::ZshHistoryParser;

#[test]
fn fixture_written_by_real_zsh_parses() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/zsh_history_extended"
    ))
    .unwrap();
    assert_eq!(
        ZshHistoryParser::new().feed(&bytes),
        [
            "echo 日本語",
            "for i in 1 2\ndo\n  echo $i\ndone",
            "git status"
        ]
    );
}
