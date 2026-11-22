//! The fish adapter in a real interactive fish on a pty (temporary HOME, empty environment).
//! The fish under test is `$FUXX_TEST_FISH`, else the newest one `scripts/build-fish.sh` built.

mod common;

use common::fish::{FishHarness, FishOpts};

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
