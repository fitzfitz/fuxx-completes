//! The size budget of the release binary: under 768 KB as a whole. M1b allowed 150 KB on top of
//! M1's 502,800 bytes; M1d (safety and polish) went past that at 702,224 bytes, and the user chose
//! one limit for the whole binary instead (2026-11-22).
//! Run in release: `cargo test --release -p fuxx-completes --test size -- --ignored`.

const M1_BINARY: u64 = 502_800;
const LIMIT: u64 = 768 * 1024;

#[test]
#[ignore = "measures the release binary; run it with --release -- --ignored"]
fn release_binary_stays_within_the_size_limit() {
    let len = std::fs::metadata(env!("CARGO_BIN_EXE_fuxx-completes"))
        .unwrap()
        .len();
    println!(
        "binary: {len} bytes ({:+} from M1)",
        len as i64 - M1_BINARY as i64
    );
    assert!(len < LIMIT, "{len} bytes, over {LIMIT}");
}
