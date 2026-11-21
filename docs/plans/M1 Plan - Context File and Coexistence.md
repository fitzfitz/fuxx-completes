# M1 Context File and Coexistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Written 2026-11-21 for the user to review, after they closed M0 ("close m0, all good"). Decisions
marked **AI's call** are reasoned defaults the user can change. Each one also goes into
`docs/design.md` in the task named.

**Goal:** Ghost text ranked by where, how often and how well a command ran (across shells and
restarts, from the small context file), plus the previous command in this session. Also: staying
off beside zsh-autosuggestions, in Warp, and when a second copy loads; `forget`; `doctor`; and data
under `FUXX_PLUGIN_DATA`.

**Architecture:** A new `core/src/context/` module owns the data folder, the secret, the keyed
record lines, the in-memory tallies and compaction. A new `core/src/rank.rs` turns history plus
those tallies into one candidate order, and M0's `cd` walk runs over that order instead of over
plain recency. `cli/` wires the store into `Session`, adds the `prev` message, and adds the
`forget` and `doctor` subcommands. The zsh adapter gains the double-load guard, the conflict
checks, the data-folder copy, the `prev` report and a `fuxx-completes` shell function that hands
shell facts to `doctor`.

**Tech Stack:** Rust 2021, toolchain 1.97.1 (std `File::try_lock`/`try_lock_shared`, stable since
1.89, checked on this machine 2026-11-21). New runtime dependency: `blake3 = { version = "1.8.7",
default-features = false, features = ["pure"] }` (Context7 `/websites/rs_blake3`:
`keyed_hash(&[u8; 32], &[u8]) -> Hash`; `pure` builds no C or assembly; inputs are short, so SIMD
doesn't matter). zsh 5.3+. Dev crates as in M0 (`expectrl` 0.9, `tempfile` 3.27).

**Spec:** `docs/design.md` §3, §5.4, §5.5, §5.7, §5.8, §7, §8, §9, §13, §14, §15 (M1 row), §16
(`shglob`). Rules: `AGENTS.md`.

## Decisions this plan settles (design §5.7 and §7 say "details in the M1 plan")

| Question | **AI's call** | Why (one line) | Lands in |
| --- | --- | --- | --- |
| Keyed hash | BLAKE3 keyed mode (`blake3` 1.8.7, `pure`), the first 16 bytes as a `u128` `Key` | One small crate, made for keyed hashing; one call per command | Task 1, §5.7 |
| Random secret | 32 bytes read from `/dev/urandom` (std only, macOS and Linux), written with `create_new` at mode `0600` | No extra crate; `create_new` settles two cores racing to create it | Task 1, §5.7 |
| Data folder | `$FUXX_PLUGIN_DATA` if set and absolute, else `$XDG_DATA_HOME/fuxx-completes` if absolute, else `$HOME/.local/share/fuxx-completes`; empty counts as unset | §5.7 and §8 in one rule | Task 1, §5.7, §8 |
| Record encoding | One text line per record: `1 <32 hex key> <status> <time> <folder>\n`; in the folder, bytes `0x00`–`0x1F`, `0x7F` and `%` are written `%XX`. A line that doesn't decode (a torn tail, junk, another version) is skipped | A torn write damages only its own line; a reader picks up again at the next LF | Task 1, §5.7 |
| Which file a core writes | `context/<32 hex of key_of(history path)>`; a core with no history path writes no records (they could never match after a restart) | §5.7's "one file per history file" | Task 1, §5.7 |
| Size cap | 4 MiB per file; compaction trims the oldest down to 3 MiB | About 60,000 records; the gap stops a compaction at every append | Task 2, §5.7 |
| Lock | A `context/<id>.lock` file. Appends take `try_lock_shared`, compaction `try_lock`. **Nobody waits:** a busy append stays pending in memory (at most 1,000) and is retried at the next request; a busy compaction is skipped | An append can never land in a file that is being replaced, and the serve loop never blocks (rule 6) | Tasks 1–2, §5.7 |
| Reading context files | Every file in `context/` is read when the store opens, then checked at every request: growth is read inline; any other change rebuilds the whole index on a worker thread, and the old index serves until then | The same pattern as M0's history file | Task 2, §5.7 |
| `forget` seen by running cores | Each request checks the `key` file's device and inode. If it is gone or changed, the core drops its secret, index, pending records and history keys | §5.7's "every running core sees the files are gone" | Task 4, §5.7 |
| Ranking weights | The order in "Ranking order" below | Deterministic and easy to test; frequency is the user's 2026-11-21 decision | Task 3, §5.4 |
| Previous command | A new no-reply message `prev` (one field: the command the adapter holds back to report, after its filters; empty = unknown). Successors are the pairs of consecutive `ran` reports in this session, in memory only | `ran` comes one prompt late (§5.8), so the core's last `ran` is never the command just run | Tasks 4–5, §5.4, §5.5 |
| `SHARE_HISTORY` limit (§5.8, "fixed in M1") | A `ran` whose text equals a file entry read since the previous `ran` retires that entry's slot and adds no session record | Stops a session record outranking newer file entries from other shells | Task 4, §5.8 |
| Conflict checks | At every `precmd` (cheap lookups): Warp (`TERM_PROGRAM=WarpTerminal`) → off; `_zsh_autosuggest_start` defined → off unless `FUXX_AUTOSUGGESTIONS_FORCE=1`. If it is found after the core started, the core is stopped | Covers plugins loaded late (zinit turbo, zsh-defer) | Task 5, §9 |
| Double-load guard | The first copy keeps its resolved file path in `_FUXX_AS_FILE`. Sourcing **another** path adds that path to `_FUXX_AS_SECOND` and stops before defining any function. Sourcing the same path again works as in M0 | The second copy's functions would replace the first's | Task 5, §8 |
| How `doctor` and `forget` see shell facts and fuxx's data folder | The adapter defines a shell function `fuxx-completes` (only if no function or alias of that name exists). It runs the binary with `FUXX_PLUGIN_DATA` set to the adapter's copy and, for `doctor`, adds `--shell-facts <lines>` | fuxx unsets `FUXX_PLUGIN_DATA` after sourcing, so a plain `fuxx-completes forget` in fuxx would wipe the wrong folder | Tasks 5–6, §7 |
| "The core's last error" in `doctor` | The adapter's reason for going off (`_FUXX_AS_WHY`: `no-binary`, `start-failed`, `hung`, `exited`, `protocol`, `write-failed`, `conflict:warp`, `conflict:zsh-autosuggestions`) | Only the shell sees why its core stopped | Tasks 5–6, §7 |
| `shglob` (§16) | Keep `add-zle-hook-widget` (AGENTS.md says so). `doctor` warns when `shglob` is on and names zsh's dispatcher bug | We can't fix zsh's function; we can explain the message | Task 6, §16 |

### Ranking order (Task 3 implements it; design §5.4 gets this text)

For a typed prefix `P`, the request's cwd `C` and the previous command `p` (from `prev`; may be
unknown), each distinct history text `t` that starts with `P` and has key `k` gets:

- `here_ok` / `here_failed`: records of `k` in `C` exactly (byte-equal folder) with status 0 /
  non-0. `all_ok` / `all_failed`: the same in every folder.
- **Tier 1:** `here_ok ≥ 1`. **Tier 3:** "failed only", meaning `here_ok = 0` and `here_failed ≥ 1`,
  or no record in `C` and `all_ok = 0` and `all_failed ≥ 1`. **Tier 2:** everything else, which
  includes every text with no records at all.
- `follows`: `p` is known and this session's `ran` pairs hold `(p, t)`.
- `freq`: `here_ok`, but 1 when `t`'s command word is `cd`, `pushd` or `open` (after `builtin`,
  `command` and `NAME=value`; the first simple command only).

Order: tier ascending; then `follows` first; then, in tier 1 only, `freq` descending and the newest
record time in `C` descending; then M0's recency (session records newest first, then file entries
newest first). M0's `cd` walk (`Best`/`Plain`/`Hold`/`Skip`, the 50-candidate bound and the 50 ms
budget) then runs over this order instead of plain recency. **With no context (no secret, no
records, `p` unknown) the order is exactly M0's.** That is the regression guard.

The order is produced lazily: tier 1 and the `follows` texts come from small sets (the cwd's
tallies and the session's pairs). Tier 2 is M0's recency walk with tier-1, already-yielded and
failed-only texts left out; the failed-only texts are collected as the walk passes them and
yielded last. Without context, nothing beyond M0's walk runs.

## Global Constraints

- **No test reads or writes the real data folder, rc files or history.** Core tests pass folders
  explicitly; `Session` has a constructor that takes a `DataDir`; every spawned binary or zsh gets
  a temporary `HOME` and `XDG_DATA_HOME` (rule: AGENTS.md "Tests never touch…").
- The context file holds a key, a folder, an exit code and a time per line. **Never the command
  text** (rule 7, §5.7). Folder `0700`, files `0600`.
- A keystroke never waits on the core, and the core never waits on a lock (rule 6). Suggestion
  answers stay under **10 ms at p95 on a 100,000-line history** (§3).
- No network code. The only new runtime dependency is `blake3` (`pure`).
- Every adapter function starts with `emulate -L zsh`; the adapter prints nothing at startup or at
  the prompt (only the user-run `fuxx-completes` function prints); hooks only through
  `add-zsh-hook` / `add-zle-hook-widget`.
- Per-task gate: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --check`. Adapter tasks also run `scripts/test-zsh-matrix.sh` (zsh 5.3.1, 5.8.1, 5.9).
- Docs with every task (rule 4): `docs/design.md` sections named in the table, and a Progress log
  entry in `docs/roadmap.md`.
- Commit on branch `m1-context-file-coexistence`, only when the user asks.

## Review Focus

1. **The data folder can't be written** (read-only, full disk, a file where the folder should
   be): suggestions keep coming from history, `ran` is dropped silently, nothing reaches the
   prompt. Test in Task 4.
2. **`forget` run from another shell while a core is serving**: the next answer ranks as if there
   were no context, an old record never matches again, and the next `ran` makes a new secret.
   Test in Task 4.
3. **A damaged context file** (torn last line, binary junk, a line from a newer version, a
   16 MB file from an old bug): bad lines are skipped, good ones are used, and an oversized own
   file is compacted. Tests in Tasks 1–2.
4. **Folder names with spaces, `%`, newlines, tabs or non-UTF-8 bytes** round-trip exactly and
   never split a line. Test in Task 1.
5. **Two shells on one history file** append to the same context file while one compacts: no line
   is lost or garbled except as §5.7 allows (records of commands not in history). Test in Task 2.

---

## File map

| File | What it holds |
| --- | --- |
| `core/src/context/mod.rs` (new) | `Key`, `DataDir`, `DataSource`, `data_dir`, re-exports |
| `core/src/context/secret.rs` (new) | `Secret`: load, create, `key_of` |
| `core/src/context/record.rs` (new) | `Record`, `encode`, `decode` |
| `core/src/context/index.rs` (new) | `Tally`, `ContextIndex` |
| `core/src/context/store.rs` (new) | `ContextStore`: open, refresh, record, compaction, `forget` |
| `core/src/rank.rs` (new) | `Ranking`, `Successors`, the candidate order, `walk` (M0's walk moved here) |
| `core/src/cd_target.rs` | `pub fn command_word` (exposes the parse it already has) |
| `core/src/history.rs` | keys per entry, `ranked_with_prefix`, the `SHARE_HISTORY` retire rule |
| `core/src/history_file.rs` | `has_loaded()` |
| `core/src/protocol.rs` | the `prev` message |
| `fuxx-autosuggestions/src/lib.rs` | `suggestion()` takes a `&Ranking` |
| `cli/src/session.rs` | the store, successors, `prev`, compaction trigger |
| `cli/src/serve.rs`, `cli/src/main.rs` | `serve_with(Session, …)`; `forget`, `doctor` subcommands |
| `cli/src/doctor.rs` (new) | the doctor report |
| `cli/tests/latency.rs` (new) | the §3 benchmark (`#[ignore]`, run in release) |
| `cli/tests/cli.rs` (new) | `forget` and `doctor` as real processes |
| `adapters/zsh/fuxx-autosuggestions.plugin.zsh` | guard, conflicts, data copy, `prev`, `_FUXX_AS_WHY`, key outcomes, `fuxx-completes` function |
| `cli/tests/zsh_adapter.rs` | the adapter tests below |
| `docs/design.md`, `docs/roadmap.md`, `README.md` | per task |

---

### Task 1: Data folder, secret, keyed records

**Files:**
- Create: `core/src/context/{mod.rs,secret.rs,record.rs}`, `core/src/context/store.rs` (append and `forget` only; Task 2 adds the rest)
- Modify: `core/Cargo.toml` (`blake3`), `core/src/lib.rs` (`pub mod context;`)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)] pub struct Key(pub u128);` with `fn to_hex(&self) -> String` (32 lowercase hex) and `fn from_hex(&str) -> Option<Key>`
  - `pub enum DataSource { PluginData, Xdg, Home }`, `pub struct DataDir { pub root: PathBuf, pub source: DataSource }`
  - `pub fn data_dir(plugin_data: Option<&OsStr>, xdg_data_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<DataDir>` (pure) and `pub fn data_dir_from_env() -> Option<DataDir>`
  - `pub struct Secret([u8; 32])`: `fn load(root: &Path) -> io::Result<Option<Secret>>`, `fn load_or_create(root: &Path) -> io::Result<Secret>`, `fn key_of(&self, text: &str) -> Key`
  - `pub struct Record { pub key: Key, pub folder: PathBuf, pub status: i32, pub time: i64 }`, `pub fn encode(&Record) -> Vec<u8>` (with the LF), `pub fn decode(line: &[u8]) -> Option<Record>` (without the LF)
  - `pub fn own_file_id(secret: &Secret, history_path: &Path) -> String` (the hex of `key_of` of the path's bytes)
  - `pub fn append(root: &Path, id: &str, records: &[Record]) -> Result<(), AppendError>` where `AppendError::{Busy, Io(io::Error)}`: makes `context/` (`0700`), takes `try_lock_shared` on `context/<id>.lock`, opens `context/<id>` with `O_APPEND` (create `0600`), and writes all lines in **one** `write_all`
  - `pub fn forget(root: &Path, remove_root_if_empty: bool) -> io::Result<bool>` (true = something was removed): deletes `context/` with everything in it and `key`
  - `pub const RECORD_VERSION: u8 = 1;`

- [ ] **Step 1: Context7 check.** `resolve-library-id blake3` → `/websites/rs_blake3`, `query-docs` for `keyed_hash` and the `pure` feature. Confirm 1.8.7 is still current (`cargo search blake3 --limit 1`).
- [ ] **Step 2: Write the failing tests** (in each new file's `mod tests`, using `tempfile`):
  - `data_dir_prefers_plugin_data_then_xdg_then_home`: `(Some("/p"), Some("/x"), Some("/h"))` → `/p`, `PluginData`; `(None, Some("/x"), Some("/h"))` → `/x/fuxx-completes`, `Xdg`; `(None, None, Some("/h"))` → `/h/.local/share/fuxx-completes`, `Home`; `(Some(""), Some("rel"), Some("/h"))` → the `Home` one (empty and relative count as unset); all `None` → `None`.
  - `secret_is_created_once_with_0600_in_a_0700_folder`: `load` on an empty folder → `None`; `load_or_create` twice gives equal secrets; modes of `root` and `root/key` are `0o700` and `0o600`; the file is 32 bytes.
  - `secret_from_a_short_key_file_is_an_error`: a 5-byte `key` → `load` is `Err`.
  - `key_of_is_keyed`: two different secrets give different keys for `"ls"`; one secret gives the same key twice; `Key::from_hex(&k.to_hex()) == Some(k)`.
  - `record_round_trips_odd_folders`: folders `/a b`, `/50%`, `/x\ny`, `/t\tz`, and one made with `OsStr::from_bytes(b"/\xff\xfe")` decode back byte-equal; the encoded line has exactly one LF, at the end.
  - `decode_skips_bad_lines`: `b""`, `b"2 …"` (another version), a 31-hex key, a non-number status, a torn line without its folder, `b"\xff\xff"` → all `None`.
  - `context_file_never_holds_command_text`: append records for the commands `git push origin main`, `ls`, `secret-token-xyz` (keys from a secret); the file's bytes contain none of the three texts.
  - `append_makes_0600_file_and_0700_folder`, and `append_with_the_lock_held_exclusively_is_busy` (hold `try_lock` on the `.lock` from the test → `Err(AppendError::Busy)`, nothing written).
  - `forget_removes_context_and_key_only`: an extra file `other` in `root` survives; `forget(root, true)` on a root holding only our files removes the root; on a missing root it returns `Ok(false)`.
- [ ] **Step 3: Run `cargo test -p fuxx-completes-core context`** — expect compile errors (no module yet).
- [ ] **Step 4: Implement** the signatures above. `key_of` = the first 16 bytes of `blake3::keyed_hash(&self.0, text.as_bytes())` as a big-endian `u128`. `load_or_create`: `create_dir_all` and then `set_permissions(0o700)` on `root`, read `/dev/urandom` for 32 bytes, `OpenOptions::new().write(true).create_new(true).mode(0o600)`; on `AlreadyExists`, `load` again. The folder is written with `OsStrExt::as_bytes` and %-escaped as in the decisions table.
- [ ] **Step 5: Run the tests again** — PASS. Run the per-task gate.
- [ ] **Step 6: Docs.** `design.md` §5.7: the key (BLAKE3 keyed, 128 bits), `/dev/urandom`, the line format, the data-folder rule, "no history path, no records". §8: the data-folder order. Progress log entry.
- [ ] **Step 7: Commit** (when the user asks): `feat(core): context records — data folder, keyed secret, record lines, append, forget`

### Task 2: Reading, indexing and compacting the context files

**Files:**
- Create: `core/src/context/index.rs`
- Modify: `core/src/context/store.rs`

**Interfaces:**
- Consumes: Task 1's `Record`, `decode`, `append`, `Secret`, `Key`, `own_file_id`.
- Produces:
  - `#[derive(Default, Clone, Copy)] pub struct Tally { pub ok: u32, pub failed: u32, pub last_time: i64 }`
  - `pub struct ContextIndex`: `fn add(&mut self, r: &Record)`, `fn in_folder(&self, folder: &Path) -> Option<&HashMap<Key, Tally>>`, `fn anywhere(&self, key: Key) -> Option<Tally>`, `fn is_empty(&self) -> bool`
  - `pub struct ContextStore`:
    - `fn open(dir: DataDir, history_path: Option<PathBuf>) -> ContextStore`: never fails; it loads the secret if there is one and reads every file in `context/`
    - `fn refresh(&mut self)`: checks the key file (Task 4 uses the result), reads growth inline, starts a background rebuild on any other change (a new file, a shrink, a new inode), swaps a finished rebuild in, and retries pending appends
    - `fn record(&mut self, command: &str, folder: &Path, status: i32, time: i64)`: no-op without a history path; creates the secret on the first record; appends; `Busy` or an I/O error keeps the record pending (at most 1,000, oldest dropped); a pending record already counts in the index
    - `fn secret(&self) -> Option<&Secret>`, `fn index(&self) -> &ContextIndex`, `fn dir(&self) -> &DataDir`
    - `fn compact_in_background(&mut self, keep: HashSet<Key>)`: own file only, on a worker thread; if `try_lock` is busy it does nothing; it keeps the records whose key is in `keep`; if the result is still over `CAP`, it drops the oldest (by `time`, then file order) until it is at or under `TRIM_TO`; it writes `context/<id>.tmp-<pid>` (`0600`) and renames it over the file
    - `fn own_file_len(&self) -> u64`
  - `pub const CAP: u64 = 4 * 1024 * 1024; pub const TRIM_TO: u64 = 3 * 1024 * 1024;`

- [ ] **Step 1: Write the failing tests** (`store.rs` tests, all in temp folders):
  - `open_reads_every_context_file`: two files (two history paths) with records → `index().anywhere(k)` sums both; `in_folder` splits by folder; `last_time` is the max.
  - `refresh_reads_growth_inline`: append a line from outside after `open`; one `refresh` → it is counted (no thread wait).
  - `refresh_rebuilds_after_a_rename_and_swaps_when_done`: replace a file through a rename; `refresh` until the background rebuild lands (poll ≤ 2 s) → the index matches the new file; while it runs, the old index still answers.
  - `damaged_lines_are_skipped`: a file with good, junk and torn lines → only good ones counted; a torn last line is read once it is completed by a later append (keep the partial bytes between refreshes, as `history_file` does).
  - `record_without_history_path_writes_nothing`; `record_creates_the_secret_on_first_use`.
  - `busy_append_stays_pending_and_lands_later`: hold the exclusive lock, `record` → the file is empty but `index()` counts it; release, `refresh` → the line is in the file, once.
  - `compaction_drops_unknown_keys_and_trims_oldest`: records for keys A, B and unknown C, and a file pushed over `CAP` with old A records → after compaction no C line is left, the size is ≤ `TRIM_TO`, the newest A and B records are kept, mode `0600`.
  - `compaction_skips_when_locked`: hold `try_lock_shared` from the test → the file is unchanged.
  - `appends_during_compaction_are_not_lost` (Review Focus 5): two threads each `append` 2,000 records for kept keys while a third runs `compact_in_background` in a loop; at the end each of the 4,000 lines is present exactly once (no torn lines: every line decodes).
  - `oversized_file_at_open_is_compacted_by_the_first_compaction` (Review Focus 3): a 16 MiB own file → after one compaction it is ≤ `TRIM_TO`.
- [ ] **Step 2: Run `cargo test -p fuxx-completes-core context::store`** — FAIL (missing items).
- [ ] **Step 3: Implement.** A per-file state (`dev`, `ino`, consumed length, partial tail) like `history_file::Stamp`; the rebuild thread returns a fresh `ContextIndex` and the new per-file states; pending records are folded into the index at once and dropped from the pending list once written.
- [ ] **Step 4: Run the tests** — PASS; per-task gate.
- [ ] **Step 5: Docs.** §5.7 "Pruning" and "Concurrent writers": the cap, the trim target, the lock file, try-locks, pending, background rebuild. Progress log.
- [ ] **Step 6: Commit** (when asked): `feat(core): context index, refresh and compaction under a try-lock`

### Task 3: The ranking order

**Files:**
- Create: `core/src/rank.rs`
- Modify: `core/src/history.rs`, `core/src/cd_target.rs`, `core/src/lib.rs`, `fuxx-autosuggestions/src/lib.rs`

**Interfaces:**
- Consumes: `Key`, `Secret::key_of`, `ContextIndex`, `Tally` (Tasks 1–2); M0's `Verdict`, `MAX_SKIPPED`, `CHECK_BUDGET`.
- Produces:
  - `cd_target::command_word(command: &str) -> Option<String>`: the first simple command's word after `builtin`/`command` and `NAME=value` (M0's `first_simple_command` parse; unquoted)
  - `rank::no_frequency_boost(command: &str) -> bool`: `command_word` is `cd`, `pushd` or `open`
  - `rank::Successors`: `fn new() -> Self`, `fn record(&mut self, prev: &str, next: &str)` (at most 10,000 pairs, oldest dropped), `fn follows(&self, prev: &str, next: &str) -> bool`
  - `rank::Ranking<'a> { pub index: Option<&'a ContextIndex>, pub cwd: &'a Path, pub previous: Option<&'a str>, pub successors: Option<&'a Successors> }` and `Ranking::none(cwd) -> Ranking`
  - `History::set_secret(&mut self, secret: Option<&Secret>)`: recomputes every entry's key (or clears them); new entries are keyed as they arrive
  - `History::ranked_with_prefix<'a>(&'a self, prefix: &str, ranking: &Ranking<'_>) -> impl Iterator<Item = &'a str> + 'a`: distinct texts in the "Ranking order" above
  - `rank::walk<'a>(candidates: impl Iterator<Item = &'a str>, budget: Duration, judge: impl FnMut(&str) -> Verdict) -> Option<&'a str>`: M0's `best_with_prefix_within` body, moved here unchanged; `History::best_with_prefix*` and `latest_with_prefix*` become thin calls over the recency order
  - `fuxx_autosuggestions::suggestion<'a>(history: &'a History, buffer: &str, place: &Place, ranking: &Ranking) -> Option<&'a str>`

- [ ] **Step 1: Write the failing tests** (`rank.rs`; a helper builds a `History` with a secret plus a `ContextIndex` from `(command, folder, status, time)` tuples):
  - `no_context_order_is_m0s`: same history, `Ranking::none` → `ranked_with_prefix` yields exactly what M0's recency walk yields (session newest first, then file newest first, distinct).
  - `ran_here_successfully_beats_newer_history`: file `["make test", "make build"]`, records `make test` ok in `/p` → in `/p`, `"make "` gives `make test` first; in `/q`, `make build`.
  - `frequency_here_orders_tier_one`: `git status` ok ×3 in `/p`, `git stash` ok ×1 (newer) → `git status` first; ties go to the newest record time in `/p`.
  - `cd_pushd_open_get_no_frequency_boost`: `cd src` ok ×9 in `/p` and `cd lib` ok ×1 with a newer record time (both folders exist under `/p` in a temp tree) → `cd lib` first. The same for `open a.txt` against `open b.txt`, and for `builtin cd x` / `FOO=1 pushd y`. `no_frequency_boost("cdk deploy") == false`.
  - `failed_only_ranks_last`: `gti status` failed in `/p`, `git status` with no records → `git status` first. A command that only ever failed elsewhere and has no record here ranks below one with no records at all. One that failed here but succeeded once here is tier 1.
  - `follows_previous_ranks_first_within_its_tier`: successors `("git add .", "git commit")`, previous `git add .` → `git commit` above a newer `git checkout` (both tier 2); with previous unknown, recency wins.
  - `ranked_order_feeds_the_cd_walk`: `cd ..` ok ×5 in `/p` (tier 1, `Hold`) and `cd proj` with no records (`Best`) → `suggestion` gives `proj` (M0's rule still holds over the new order). A `Skip` (missing folder) tier-1 `cd` is passed over.
  - `forget_like_secret_change_drops_keys`: `set_secret(None)` → the order is M0's again.
  - Every existing `history.rs` and `fuxx-autosuggestions` test still passes, unchanged except for the added `&Ranking::none(..)` argument.
- [ ] **Step 2: Run `cargo test -p fuxx-completes-core rank` and `-p fuxx-autosuggestions`** — FAIL.
- [ ] **Step 3: Implement** as described under "Ranking order". `History` keeps one key per entry plus a `HashMap<Key, newest position>` so tier-1 texts are found from the cwd's tallies without a scan.
- [ ] **Step 4: Run the tests** — PASS; per-task gate.
- [ ] **Step 5: Docs.** §5.4: replace "The weights are the builder's choice" with the "Ranking order" text (AI's call). Progress log.
- [ ] **Step 6: Commit** (when asked): `feat(core): rank by folder, success, frequency (no boost for cd/pushd/open) and the previous command`

### Task 4: The session: store, `prev`, `forget` seen live, `SHARE_HISTORY`, benchmark

**Files:**
- Modify: `core/src/protocol.rs`, `core/src/history.rs`, `core/src/history_file.rs`, `cli/src/session.rs`, `cli/src/serve.rs`, `cli/src/main.rs`, `cli/tests/serve.rs`
- Create: `cli/tests/latency.rs`

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces:
  - `Request::Prev { command: Option<String> }` (`prev` + one field; empty → `None`); encode/decode both ways
  - `Session::new(data: Option<DataDir>, home: Option<PathBuf>) -> Session`; `Session::from_env() -> Session` (used only by `main`)
  - `serve::serve_with(session: Session, input, output) -> io::Result<()>`; `serve::serve` stays for `main` and calls `serve_with(Session::from_env(), …)`
  - `HistoryFile::has_loaded(&self) -> bool`: true once the first full read was applied
  - `History::note_ran(&mut self, command: String)` replaces `record_session` for `ran`: the `SHARE_HISTORY` rule from the decisions table

Session behaviour:
- `hello`: opens the `ContextStore` (with the history path) and gives `History` the secret.
- `ran`: `store.record(…)`, `successors.record(last_ran, command)`, `last_ran = command`, `history.note_ran(command)`; if the secret was just created, `history.set_secret`.
- `prev`: stores `previous`.
- `suggest`: `store.refresh()`. If the key file is gone or changed, drop the secret, index, pending records and history keys. Then `history_file.refresh`, and if `has_loaded()` and this core hasn't compacted yet (or the own file is over `CAP`), `compact_in_background(history keys)`. Then rank with `Ranking { index, cwd, previous, successors }`.

- [ ] **Step 1: Write the failing tests:**
  - protocol: `prev_round_trips` (`prev␟git add .`), `empty_prev_is_unknown`, `prev_from_an_old_core_is_ignored` (an unknown kind still decodes as `UnknownKind`, for M0 cores: already true; keep a test).
  - `history.rs`: `ran_already_in_the_file_adds_no_session_record` (append file entries `["make"]`, then `note_ran("make")` → `session_len() == 0`, and a newer file entry from another shell, `make x`, wins for `"make"`); `ran_not_yet_in_the_file_is_a_session_record` (the M0 behaviour).
  - session (all with `Session::new(Some(tmp DataDir), Some(tmp home))`):
    - `context_survives_a_restart`: session 1 gets `hello` with a temp `HISTFILE` holding `make build` and `make test`, then `ran make test` ×2 in `/p`, then ends. Session 2 starts on the same data folder → in `/p`, `"make "` suggests `test` although `build` is newer.
    - `prev_message_drives_follows`: `ran git add .`, `ran git commit -m x`, `prev git add .` → `"git c"` suggests `ommit -m x` over a newer file entry `git checkout main`.
    - `forget_while_serving_drops_context` (Review Focus 2): after records, call `context::store::forget` on the folder; the next `suggest` ranks like M0; the next `ran` creates a new key file and its records count.
    - `unwritable_data_folder_still_suggests` (Review Focus 1): the data root is a regular file → `hello`, `ran`, `suggest` all answer as M0 would, with no panic.
    - `first_full_read_triggers_one_compaction`: a context file with a record for a command not in history → after the history loads (poll `suggest` ≤ 2 s), that record is gone from the file.
  - Existing `session.rs`, `serve.rs` and `cli/tests/serve.rs` tests move to temp data folders: in-process tests use `Session::new(Some(tmp), …)`; spawned binaries get `HOME` and `XDG_DATA_HOME` in a temp folder.
  - `cli/tests/latency.rs` (`#[ignore]`): `suggest_p95_under_10_ms_on_100k_history`. A temp `HISTFILE` with 100,000 generated lines (about 5,000 distinct commands over 50 tools, args varied) and a context file with 50,000 records over 200 folders. After `hello`, wait until `has_loaded`, then time 2,000 `Session::handle(suggest)` calls with prefixes of 1–6 characters drawn from history and cwds drawn from the record folders. Print p50/p95/max; assert p95 < 10 ms. Also time one `hello`-to-loaded on the same data and print it (reported, not asserted).
- [ ] **Step 2: Run `cargo test --workspace`** — FAIL on the new tests.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace` — PASS — and `cargo test --release -p fuxx-completes --test latency -- --ignored --nocapture` — p95 < 10 ms. If it misses, profile before changing anything (`superpowers:systematic-debugging`); don't loosen the assert.
- [ ] **Step 5: Docs.** §5.5: the `prev` row in the wire table. §5.7: `forget` seen through the key file's inode. §5.8: replace "That is ranking only, to be fixed in M1" with the retire rule. §3: the measured p50/p95. Progress log.
- [ ] **Step 6: Commit** (when asked): `feat(cli): context-aware sessions — prev, live forget, SHARE_HISTORY retire, latency benchmark`

### Task 5: zsh adapter: guard, conflicts, data folder, `prev`, facts

**Files:**
- Modify: `adapters/zsh/fuxx-autosuggestions.plugin.zsh`, `cli/tests/zsh_adapter.rs` (and `cli/tests/common/mod.rs` if a harness option is needed: an `env: Vec<(&str, String)>` for `TERM_PROGRAM`, `FUXX_PLUGIN_DATA`)

**Interfaces:**
- Consumes: the `prev` message (Task 4); the core reads `FUXX_PLUGIN_DATA` (Task 1's `data_dir_from_env`).
- Produces (shell globals that Task 6's `doctor` facts read):
  - `_FUXX_AS_FILE` (resolved path of the first copy), `_FUXX_AS_SECOND` (array of other copies' paths)
  - `_FUXX_AS_DATA` (the `FUXX_PLUGIN_DATA` seen when the first copy loaded; may be empty)
  - `_FUXX_AS_WHY` (the reason the adapter is off, from the decisions table; empty while on)
  - `_FUXX_AS_KEYS` (assoc: `"<keymap> <seq>"` → `bound` or `kept:<widget>`)
  - the function `fuxx-completes` (only if no function or alias of that name exists at load)
  - `_fuxx_as_facts` prints the facts as `name=value` lines: `shell=zsh`, `version`, `state`, `why`, `file`, `second` (one line each), `data`, `key` (one line per `_FUXX_AS_KEYS` entry), `shglob=on|off`, `histfile`, `force=0|1`

Behaviour:
- Load: compute `${${(%):-%x}:A}`. If `_FUXX_AS_FILE` is set and differs, append to `_FUXX_AS_SECOND` and `return 1` from the load function (the file stops at its `|| return 0`). On the first load, set `_FUXX_AS_FILE` and `_FUXX_AS_DATA=${FUXX_PLUGIN_DATA-}`.
- `precmd`, before starting or at any later prompt while on: Warp → `_FUXX_AS_WHY=conflict:warp`, off. `$+functions[_zsh_autosuggest_start]` and `FUXX_AUTOSUGGESTIONS_FORCE` ≠ `1` → `conflict:zsh-autosuggestions`, off. If the core is running, `_fuxx_as_stop kill` (and clear the ghost text). Silent.
- The core is started with `FUXX_PLUGIN_DATA=$_FUXX_AS_DATA` in its environment.
- When a new command becomes pending (`_FUXX_AS_PEND_*` set) send `prev <entry>`; when a line was entered but nothing became pending, send `prev` with an empty field.
- Every path that turns the adapter off sets `_FUXX_AS_WHY`. `_fuxx_as_bind_if_free` records its outcome in `_FUXX_AS_KEYS`.

- [ ] **Step 1: Write the failing tests** (real zsh on the pty, temp HOME):
  - `second_copy_stays_off_and_first_keeps_working`: copy the plugin file to `$HOME/copy.plugin.zsh`, source it in `rc_after` → ghost text still works, `_FUXX_AS_SECOND` holds the copy's path, and the first file's functions are untouched (compare `$functions[_fuxx_as_start]` before and after sourcing the copy, with a marker comment added to the copy).
  - `same_copy_sourced_again_is_fine` (M0's re-source test still passes).
  - `zsh_autosuggestions_present_keeps_us_off`: `rc_before` defines `_zsh_autosuggest_start() { }` → no core process (`core_pid` empty), `_FUXX_AS_WHY == conflict:zsh-autosuggestions`, typing `git` shows no POSTDISPLAY. With `FUXX_AUTOSUGGESTIONS_FORCE=1` → ghost text works.
  - `zsh_autosuggestions_loaded_later_stops_us`: define the function at the prompt (`run("_zsh_autosuggest_start() { }")`) → after the next prompt, the core process is gone (poll `alive`).
  - `warp_keeps_us_off`: `TERM_PROGRAM=WarpTerminal` → off, even with `FUXX_AUTOSUGGESTIONS_FORCE=1`.
  - `plugin_data_folder_is_used`: `FUXX_PLUGIN_DATA=$HOME/pdata` set in `rc_before` and unset after sourcing (as fuxx does); run `make x` then `true` → `$HOME/pdata/context/` holds one file and `$HOME/.local/share/fuxx-completes` doesn't exist.
  - `leading_space_command_never_makes_a_record` (§14): `setopt histignorespace`; run ` secret-cmd` then `true` then `true` → the context file never contains the text, and it has no line beyond the one for the first `true`.
  - `prev_is_sent_for_the_pending_command`: with the recorder core (`core_script`), run `git add .` → the wire log has `prev␟git add .` before the next `suggest`.
  - `fzf_style_bindings_keep_working`: stub `fzf-history-widget` on `^R` and `fzf-completion` on Tab in `rc_before` (each sets a marker variable) → `^R` runs the stub; Tab with no ghost text runs the stub (M0's Tab rule, with a stub fzf).
  - `facts_list_bound_and_kept_keys`: `rc_before` binds `^[[F` to `beginning-of-line` → `_fuxx_as_facts` (run at the prompt) prints `key=emacs ^[[F kept:beginning-of-line` and `key=emacs ^[[4~ bound`.
- [ ] **Step 2: Run** `cargo test -p fuxx-completes --test zsh_adapter` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `scripts/test-zsh-matrix.sh` — all pass on 5.3.1, 5.8.1, 5.9; per-task gate.
- [ ] **Step 5: Docs.** §8: the guard (path identity, stops before defining functions); §9: checks at every `precmd`, `FUXX_AUTOSUGGESTIONS_FORCE`, stopping a running core; §5.5: `prev`, and the core's env. Progress log.
- [ ] **Step 6: Commit** (when asked): `feat(zsh): double-load guard, conflict checks, plugin data folder, prev reports, shell facts`

### Task 6: `forget` and `doctor`

**Files:**
- Create: `cli/src/doctor.rs`, `cli/tests/cli.rs`
- Modify: `cli/src/main.rs`, `cli/src/lib.rs`, `adapters/zsh/fuxx-autosuggestions.plugin.zsh` (the `fuxx-completes` function), `cli/tests/zsh_adapter.rs`

**Interfaces:**
- Consumes: `data_dir_from_env`, `forget`, `CAP` (Tasks 1–2); `_fuxx_as_facts` (Task 5).
- Produces:
  - `fuxx-completes forget`: removes `context/` and `key` (the root too only when its source isn't `PluginData` and it's then empty). It prints `Removed the context files in <root>.` or `Nothing to remove in <root>.` Exit code 0, or 1 with a one-line error on I/O failure.
  - `fuxx-completes doctor [--shell-facts <lines>]`: prints the report below. Exit code 0.
  - `doctor::report(facts: Option<&str>, data: Option<&DataDir>, version: &str) -> String` (pure apart from reading files under `data`)
  - usage text for anything else: `usage: fuxx-completes serve | forget | doctor`, exit code 2.
  - zsh function `fuxx-completes() { … }` as in the decisions table.

Report (one fact per line, in this order; missing facts are left out):
```
fuxx-completes <version>
shell        zsh <version>
state        on | off (<why>)
copy         <file>   [second copy stayed off: <path>, one line each]
keys         <keymap> <seq> bound | kept (<widget>)   [one line each]
host mode    not built yet (M2)
data         <root> (from FUXX_PLUGIN_DATA | XDG_DATA_HOME | HOME)
context      <n> files, <size>; this shell's file <size> of 4 MiB | none yet
secret       present | none yet
warning      shglob is on: zsh's add-zle-hook-widget prints "No such widget" at each prompt (design §16)
```
Without `--shell-facts` the shell lines read `shell        run this inside zsh with fuxx-autosuggestions loaded for shell facts`.

- [ ] **Step 1: Write the failing tests:**
  - `doctor.rs`: `report_without_facts`, `report_with_facts_on`, `report_with_conflict_and_second_copy`, `report_warns_on_shglob`, `report_counts_context_files` (temp data folder with two files; sizes shown in KB to one decimal).
  - `cli/tests/cli.rs` (the real binary, `HOME`/`XDG_DATA_HOME` in a temp folder, `env_remove("FUXX_PLUGIN_DATA")`): `forget_removes_and_reports`, `forget_twice_says_nothing_to_remove`, `forget_with_plugin_data_keeps_the_root`, `unknown_subcommand_prints_usage_exit_2`, `doctor_runs_outside_a_shell`.
  - `zsh_adapter.rs`: `doctor_function_passes_shell_facts` (run `fuxx-completes doctor` at the prompt → the screen shows `state        on` and `keys`), `forget_function_uses_the_plugin_data_folder` (Task 5's `FUXX_PLUGIN_DATA` setup, a record made, then `fuxx-completes forget` → `$HOME/pdata/context` is gone and `$HOME/pdata` stays), `existing_function_named_fuxx_completes_is_left_alone`.
- [ ] **Step 2: Run them** — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the per-task gate and `scripts/test-zsh-matrix.sh` — PASS.
- [ ] **Step 5: Docs.** §7: the doctor report, the `fuxx-completes` function and why it exists, "last error" = `_FUXX_AS_WHY`; §16: the `shglob` warning; `README.md`: `forget`, `doctor`, `FUXX_AUTOSUGGESTIONS_FORCE`, what the context file holds. Progress log.
- [ ] **Step 6: Commit** (when asked): `feat(cli): forget and doctor; the zsh fuxx-completes function hands doctor the shell's facts`

### Task 7: Whole-branch check, the user's hand check, closing M1's docs

- [ ] **Step 1:** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `scripts/test-zsh-matrix.sh`, `cargo test --release -p fuxx-completes --test latency -- --ignored --nocapture`. Paste the real output into the Progress log entry.
- [ ] **Step 2:** One whole-branch review on the most capable model (`superpowers:requesting-code-review`), one fix wave, Step 1 again.
- [ ] **Step 3: The user's check in a real terminal** (fuxx and one other terminal), via a local build installed as a fuxx plugin or with `FUXX_COMPLETES_BIN`. Run a command a few times in one folder and see it ranked first there but not in another folder. Run `fuxx-completes doctor`. Run `fuxx-completes forget` and see the ranking go back to recency. If zsh-autosuggestions is installed somewhere, see that we stay off. M1 is not marked ✅ until the user says so.
- [ ] **Step 4: Docs.** Roadmap: M1 ✅ (after Step 3); `design.md` status line; Progress log. M1b's plan is written next, from its spec.

## Self-review (done 2026-11-21)

- **Spec coverage (§15 M1 row):** context file with keyed records, pruning and `forget` → Tasks
  1, 2, 4, 6. Ranking by cwd, exit status and frequency (no boost for `cd`/`pushd`/`open`), across
  shells and restarts → Tasks 3–4 (`context_survives_a_restart`; every file in `context/` is
  read). The previous command in-session → Tasks 3–5 (`prev`). Conflicting plugins and Warp →
  Task 5. `doctor` → Task 6. Latency benchmark → Task 4. `FUXX_PLUGIN_DATA` and the double-load
  guard → Tasks 1, 5, 6. §14's context-file tests (no command text, pruning, `forget` seen by
  running cores, modes, concurrent appends) → Tasks 1, 2, 4. §16 `shglob` revisit → Task 6.
  §5.8's M1 fix → Task 4.
- **Not in M1, on purpose:** bash/fish (`report` is M2), host mode (M2; doctor says so),
  zsh-autocomplete detection (it gates fuxx-autocomplete, M6), project knowledge (M1b).
- **Types:** `Key`, `Secret`, `Record`, `DataDir`, `ContextIndex`, `Tally`, `ContextStore`,
  `Ranking`, `Successors` are each defined once (Tasks 1–3) and used under those names after.
- **Proportion:** no function bodies beyond the hash truncation and the order rule, which the
  signatures don't determine.

## For the user before execution

Two choices worth your look:
1. **The `fuxx-completes` shell function** (Tasks 5–6). Without it, `fuxx-completes forget` typed in
   a fuxx terminal would wipe the non-fuxx folder (fuxx unsets `FUXX_PLUGIN_DATA` after
   sourcing), and `doctor` couldn't see the shell. The alternative is a differently named function
   (`fuxx-autosuggestions-doctor`), which leaves `forget` with the fuxx problem.
2. **The ranking order** above: a command run successfully in this folder beats a newer one that
   wasn't. A command that only ever failed goes last.
