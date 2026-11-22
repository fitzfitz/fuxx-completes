//! The bash adapter in a real interactive bash on a pty (temporary HOME, empty environment).
//! The bash under test is `$FUXX_TEST_BASH`, else the newest one `scripts/build-bash.sh` built.

mod common;

use common::bash::{BashHarness, BashOpts};

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
