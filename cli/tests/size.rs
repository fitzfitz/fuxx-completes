//! The size budget of project knowledge (M1b spec §4): at most 150 KB on top of the 502,800-byte
//! binary M1 left. Run in release: `cargo test --release -p fuxx-completes --test size -- --ignored`.

const M1_BINARY: u64 = 502_800;
const BUDGET: u64 = 150 * 1024;

#[test]
#[ignore = "measures the release binary; run it with --release -- --ignored"]
fn release_binary_stays_within_the_project_knowledge_budget() {
    let len = std::fs::metadata(env!("CARGO_BIN_EXE_fuxx-completes"))
        .unwrap()
        .len();
    println!(
        "binary: {len} bytes ({:+} from M1)",
        len as i64 - M1_BINARY as i64
    );
    assert!(
        len < M1_BINARY + BUDGET,
        "{len} bytes, over {}",
        M1_BINARY + BUDGET
    );
}
