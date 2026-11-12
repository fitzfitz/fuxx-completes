# M0 Core, History and zsh Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Written 2026-11-13 for the user to review. Decisions marked **AI's call**
are reasoned defaults the user can change; each one also lands in `docs/design.md` in the task named.

**Approved by the user 2026-11-13** ("yes, go with b and start M0 subagent-driven"): option (b) for
the question at the end — the zsh adapter reports each command **one prompt later**, after zsh has
dropped the lines it doesn't keep (Task 8, design §5.8, **the user's decision**). Same day, before
Task 1: design §8's manifest names `shell/fuxx-autosuggestions.plugin.zsh` (§5.6's naming) and the
§5 diagram no longer says fuxx links the crates.

**Goal:** A `fuxx-completes serve` binary that answers ghost-text suggestions from a zsh history file,
and a zsh plugin that draws them as you type, without ever making a keystroke wait.

**Architecture:** The core crate (`fuxx-completes-core`) parses zsh history, keeps an in-memory index
that re-reads the file when it changes, and defines the protocol types and the shell wire format.
`fuxx-autosuggestions` turns the index into one suggestion. A new `cli/` crate (package and binary
`fuxx-completes`) runs `serve` over stdin/stdout. The zsh adapter is one sourceable file that starts
`serve` on two named pipes at the first prompt, sends one request at a time, and draws answers from
a `zle -F` handler.

**Tech Stack:** Rust 2021 (std only at runtime), zsh 5.3+ (ZLE, `add-zsh-hook`,
`add-zle-hook-widget`), dev-only crates `expectrl` 0.9 and `tempfile` 3.27.

**Spec:** `docs/design.md` (§3, §5.1, §5.4, §5.5, §5.6, §5.8, §6, §12–§15). Rules: `AGENTS.md`.

## Decisions this plan settles (§5.5 "a proposal to settle in the first plan")

| Question | **AI's call** | Why (one line) | Lands in |
| --- | --- | --- | --- |
| zsh transport | **Two named pipes** (`req`, `resp`) in a private `0700` temp folder, opened so no `open` blocks, then unlinked at once | No module load, no listen/accept race, nothing left on disk, and the same mechanism bash needs in M2 | Task 6, §5.5 |
| Encoding for shell code | **A field-separated line format** ("wire format 1", below); JSON is deferred until an embedder in another language needs it | zsh splits it with one parameter expansion; no JSON parsing in shell code (rule 5) | Task 1, §5.5, §17 Q1 |
| Where the binary lives | **A new `cli/` crate**, package and binary `fuxx-completes`, depending on the core and both libraries | The binary will serve both libraries; a `[[bin]]` inside one library would make it depend on the other | Task 5, §5.5, §12 |
| How the adapter finds the binary | `$FUXX_COMPLETES_BIN` if set, else `fuxx-completes` on `PATH`; missing means silently off | No hard-coded path: fuxx's plugin puts `bin/` on `PATH` (§8), tests point the variable at the cargo build | Task 6, §5.5 |
| Request pacing | **One `suggest` in flight per shell.** Newer edits wait; when the answer comes, a changed buffer is re-sent instead of drawn. A request unanswered for **2 s** means a hung core: the adapter stops it and stays quiet | A pipe that never holds more than one request can't fill, so a write never blocks the shell | Task 6, §5.5 |
| Size cap | No request when `BUFFER` (or a reported command) is over **1000 characters** | Keeps every message far below macOS's 16 KiB pipe buffer | Task 6, §5.5 |
| Old zsh for the matrix | **Source builds** of zsh 5.3.1 and 5.8.1 into `target/zsh/<ver>/` by `scripts/build-zsh.sh`; 5.9 is macOS's `/bin/zsh` | macOS has no containers for zsh; a source build is cached and repeatable | Task 9, §14 |
| End and ⌥→ keymaps | Bound in `emacs` and `viins` only, each only if unbound there; `vicmd` is left alone | vi users have `$` and `w` in `vicmd`; §6 names emacs defaults | Task 7, §6 |

### Wire format 1 (Task 1 implements it, Task 6 speaks it)

One message per line. `US` = byte `0x1F` separates fields, `LF` = `0x0A` ends the message. Inside a
field a newline is written as `RS` = `0x1E`; a field never otherwise contains `US` or `RS` (the
adapter doesn't send such a buffer; the core answers "no suggestion" rather than send such text).

| Direction | Message | Fields after the kind |
| --- | --- | --- |
| adapter → core | `hello` | protocol (`1`), shell (`zsh`), shell version, history format (`zsh`), history path (absolute, may be empty), libraries (comma list: `autosuggestions`) |
| adapter → core | `suggest` | id (u64), mode (`async`/`sync`), cursor (code points), cwd, buffer |
| adapter → core | `ran` | cwd (where it started), exit status (i32), time (Unix seconds), command |
| adapter → core | `bye` | none |
| core → adapter | `ready` | protocol (`1`) |
| core → adapter | `suggestion` | id, text after the buffer (empty = none) |

Unknown kinds are ignored. A `suggest` whose id parses but whose other fields don't still gets an
empty `suggestion` for that id (so the adapter's one-in-flight slot always clears). M0's core reads
`cursor`, `cwd`, `mode`, and `ran`'s cwd/status/time but doesn't use them yet (M1 ranking, M2 `sync`).

## Global Constraints

Copied from `docs/design.md` and `AGENTS.md`; every task includes them.

- zsh **minimum 5.3** (`add-zle-hook-widget`); check with `is-at-least` and stay off, silently, below 5.3.
- `region_highlight` entries tagged with `memo=` **only on zsh 5.9+**; on 5.8 and older remove only
  entries equal to the exact strings we added. Default style **`fg=8`**, configurable.
- **A keystroke never waits on the core.** Stale answers dropped by id; a missing, crashed or hung
  core means no suggestion, never a slow prompt or an error at the prompt.
- **Adapters print nothing during shell startup** and nothing at the prompt (send all stderr of our
  commands to `/dev/null`).
- Latency target: core answers a suggestion **under 10 ms at p95 on 100,000 history lines** (measured
  in M1; don't build anything that can't meet it).
- **The core never calls, embeds or depends on a shell** at runtime, and has **no network code**.
- **History files are read, never written or copied.** M0 writes no file of its own except the two
  FIFOs, which are unlinked as soon as both ends are open.
- **Suggest only commands the shell kept** (§5.8): leading space with `HIST_IGNORE_SPACE`,
  `HIST_NO_STORE`, `HISTORY_IGNORE`, a line another `zshaddhistory` hook rejects, and an alias with a
  leading space never produce a suggestion. The zsh adapter reports each command **one prompt
  later**, once zsh has dropped such lines (the user's decision, 2026-11-13, option b).
- Ranking = **parity with zsh-autosuggestions' `history` strategy**: the most recent entry that starts
  with the typed text; deterministic.
- **Hooks only through `add-zsh-hook` and `add-zle-hook-widget`**; never overwrite `precmd`,
  `preexec`, `zshaddhistory`, `zle-line-init`. Our `zshaddhistory` hook always returns 0.
- **Never rebind a key the user or a plugin bound.** Never bind Tab, ⌃R, ⌃T or ↑.
- Every adapter function starts with `emulate -L zsh` (read user options such as `histignorespace`
  from `$options` *before* that line).
- **Tests never touch the real rc files, history or data**: temporary `HOME`, `ZDOTDIR`,
  `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `XDG_STATE_HOME`, generated history files,
  `env_clear()` on every spawned shell.
- Follow zsh-autosuggestions' approach (§10) but write our own code; if any function is copied, its
  MIT notice goes into `THIRD_PARTY_NOTICES.md`.
- Workspace: edition 2021, license `MIT OR Apache-2.0`. Runtime crates: **none** (std only, AI's call).
  Dev-only: `expectrl = "0.9"` (0.9.0, crates.io 2026-05-11) and `tempfile = "3.27"` (3.27.0,
  crates.io 2026-03-11) — re-check both through Context7 before adding (AGENTS.md).
- Logs carry no command lines; M0 has no log at all.
- Commits per task (the user's word, 2026-11-13) on branch `m0-core-history-zsh`, never on `main`,
  never pushed, and with **no `Co-Authored-By` line**.

## Review Focus

Inputs the design implies that most likely bite a real user; each has a test in the owning task.

1. **The user's `.zshrc` sets options that change shell semantics** (`KSH_ARRAYS`, `SH_WORD_SPLIT`,
   `NO_UNSET`, `WARN_CREATE_GLOBAL`): ghost text still works and nothing prints.
   → Task 6 `adapter_survives_hostile_user_options`.
2. **The core dies or hangs mid-session**: typing goes on, nothing prints, the hung process is stopped.
   → Task 6 `core_killed_mid_session_goes_quiet`, `hung_core_is_stopped_after_two_seconds`.
3. **A multi-line buffer or a pasted control byte** (`0x1F`/`0x1E`): multi-line works, the control
   byte just means no request. → Task 6 `multiline_buffer_gets_suggestion`,
   `separator_byte_in_buffer_sends_nothing`.
4. **Accept keys with the cursor mid-line**: → and ⌃E only move, the ghost text stays.
   → Task 7 `forward_char_mid_buffer_only_moves`.
5. **No `HISTFILE` (unset) or a missing file**: commands run in the session are still suggested.
   → Task 8 `no_histfile_session_commands_still_suggested`.
6. **A line hidden by something other than our own checks** (another plugin's `zshaddhistory`
   rejecting it, an alias with a leading space, the same hidden line twice): never suggested.
   → Task 8 `line_rejected_by_another_zshaddhistory_hook_is_not_suggested`,
   `alias_with_leading_space_is_not_suggested`, `repeated_hidden_line_is_not_suggested`.

---

## File map

| File | Responsibility | Task |
| --- | --- | --- |
| `core/src/protocol.rs` | Request/Response types, wire format 1 encode/decode | 1 |
| `core/src/zsh_history.rs` | zsh history bytes → commands (metafy, `EXTENDED_HISTORY`, continuations), incremental | 2 |
| `core/tests/fixtures/zsh_history_extended` | A history file written by real zsh (committed bytes) | 2 |
| `core/src/history.rs` | `History`: file entries + session records, most-recent-prefix lookup | 3 |
| `fuxx-autosuggestions/src/lib.rs` | `suggestion()`: the text after the buffer | 3 |
| `core/src/history_file.rs` | `HistoryFile`: stat, incremental read, background full re-read | 4 |
| `cli/` (`Cargo.toml`, `src/lib.rs`, `src/session.rs`, `src/serve.rs`, `src/main.rs`) | The `fuxx-completes` binary | 5 |
| `cli/tests/serve.rs` | Binary protocol tests | 5 |
| `adapters/zsh/fuxx-autosuggestions.plugin.zsh` | The zsh adapter, one self-contained file | 6–8 |
| `cli/tests/common/mod.rs` | `ZshHarness`: a real zsh in a pty with a temp HOME | 6 |
| `cli/tests/zsh_adapter.rs` | Adapter tests | 6–9 |
| `scripts/build-zsh.sh`, `scripts/test-zsh-matrix.sh` | Old zsh builds, matrix run | 9 |

---

### Task 1: Protocol types and wire format 1

**Files:**
- Create: `core/src/protocol.rs`
- Modify: `core/src/lib.rs` (add `pub mod protocol;`, keep the doc comment, drop "Nothing is implemented yet")
- Modify: `docs/design.md` §5.5, §17; `docs/roadmap.md`

**Interfaces:**
- Produces:
  ```rust
  pub const PROTOCOL_VERSION: u32 = 1;
  pub enum Mode { Async, Sync }
  pub enum Request {
      Hello { protocol: u32, shell: String, shell_version: String, history_format: String,
              history_path: Option<PathBuf>, libraries: Vec<String> },
      Suggest { id: u64, mode: Mode, cursor: usize, cwd: PathBuf, buffer: String },
      Ran { cwd: PathBuf, status: i32, time: i64, command: String },
      Bye,
  }
  pub enum Response { Ready { protocol: u32 }, Suggestion { id: u64, text: Option<String> } }
  pub enum DecodeError { UnknownKind(String), Malformed { id: Option<u64> } }
  pub fn decode_request(line: &[u8]) -> Result<Request, DecodeError>;   // line without its LF
  pub fn encode_request(req: &Request) -> Vec<u8>;                      // ends with LF
  pub fn decode_response(line: &[u8]) -> Result<Response, DecodeError>;
  pub fn encode_response(resp: &Response) -> Vec<u8>;                   // ends with LF
  ```
  All types derive `Debug, Clone, PartialEq, Eq`.

- [ ] **Step 1: Write the failing tests** in `core/src/protocol.rs` (`#[cfg(test)] mod tests`):

```rust
const US: u8 = 0x1f; // helper: fn line(parts: &[&str]) -> Vec<u8> joins with US
#[test] fn decodes_hello() {
    let r = decode_request(&line(&["hello","1","zsh","5.9","zsh","/h/.zsh_history","autosuggestions"])).unwrap();
    assert_eq!(r, Request::Hello { protocol: 1, shell: "zsh".into(), shell_version: "5.9".into(),
        history_format: "zsh".into(), history_path: Some("/h/.zsh_history".into()),
        libraries: vec!["autosuggestions".into()] });
}
#[test] fn empty_history_path_is_none() { /* same with "" → history_path: None */ }
#[test] fn decodes_suggest_and_maps_rs_to_newline() {
    let r = decode_request(&line(&["suggest","7","async","3","/tmp","for\x1edo"])).unwrap();
    assert_eq!(r, Request::Suggest { id: 7, mode: Mode::Async, cursor: 3, cwd: "/tmp".into(), buffer: "for\ndo".into() });
}
#[test] fn decodes_ran() { /* ["ran","/w","1","1794524352","false"] → Ran{cwd:"/w",status:1,time:1794524352,command:"false"} */ }
#[test] fn decodes_bye() { assert_eq!(decode_request(b"bye").unwrap(), Request::Bye); }
#[test] fn unknown_kind_is_reported() { assert_eq!(decode_request(b"menu"), Err(DecodeError::UnknownKind("menu".into()))); }
#[test] fn bad_suggest_keeps_its_id() {
    assert_eq!(decode_request(&line(&["suggest","9","async","x","/tmp","git"])), Err(DecodeError::Malformed { id: Some(9) }));
}
#[test] fn wrong_field_count_is_malformed() { assert_eq!(decode_request(&line(&["ran","/w"])), Err(DecodeError::Malformed { id: None })); }
#[test] fn non_utf8_field_is_malformed() { /* suggest with buffer bytes [0xff] → Malformed{id:Some(..)} */ }
#[test] fn encodes_suggestion_with_newline_as_rs() {
    assert_eq!(encode_response(&Response::Suggestion { id: 4, text: Some("a\nb".into()) }), b"suggestion\x1f4\x1fa\x1eb\n");
}
#[test] fn none_encodes_as_empty_text() { assert_eq!(encode_response(&Response::Suggestion { id: 4, text: None }), b"suggestion\x1f4\x1f\n"); }
#[test] fn text_containing_separators_encodes_as_none() {
    assert_eq!(encode_response(&Response::Suggestion { id: 4, text: Some("a\x1fb".into()) }), b"suggestion\x1f4\x1f\n");
}
#[test] fn encodes_ready() { assert_eq!(encode_response(&Response::Ready { protocol: 1 }), b"ready\x1f1\n"); }
#[test] fn request_round_trips() { /* each Request variant: decode_request(strip_lf(encode_request(&r))) == r */ }
#[test] fn response_round_trips() { /* empty text decodes to None */ }
```

- [ ] **Step 2: Run** `cargo test -p fuxx-completes-core protocol` — Expected: FAIL (items not defined).
- [ ] **Step 3: Implement** the Interfaces above. Split on `US` keeping empty fields; map `RS`↔`LF`
  per field; `encode_request` panics are not allowed — a field with `US`/`RS` in a request is the
  caller's bug, so `debug_assert!` it.
- [ ] **Step 4: Run** `cargo test -p fuxx-completes-core` — Expected: all PASS; `cargo clippy --workspace -- -D warnings` clean.
- [ ] **Step 5: Docs.** `docs/roadmap.md`: M0 → 🟨, "Current milestone: M0 (in progress)", Progress log
  entry "2026-…: M0 started; wire format 1 (protocol types)". `docs/design.md` §5.5: replace the
  "Proposal" encoding bullets with wire format 1 (the table above) marked **AI's call, settled in the
  M0 plan**; the JSON sentence becomes "a JSON encoding of the same messages is added when an
  embedder in another language needs one (deferred)"; change "fuxx links the crates (§8)" to
  "fuxx runs the zsh adapter as a plugin (§8)". §17 Q1: mark answered (field-separated for shells,
  JSON deferred).
- [ ] **Step 6: Commit** (on the user's word): `feat(core): protocol types and wire format 1`.

---

### Task 2: zsh history parser

**Files:**
- Create: `core/src/zsh_history.rs`, `core/tests/fixtures/zsh_history_extended`, `core/tests/zsh_history_fixture.rs`
- Modify: `core/src/lib.rs` (`pub mod zsh_history;`)

**Interfaces:**
- Produces:
  ```rust
  pub fn unmetafy(bytes: &[u8]) -> Vec<u8>;      // 0x83 b → b ^ 0x20
  #[derive(Debug, Default, Clone)]
  pub struct ZshHistoryParser { /* pending bytes of an incomplete entry */ }
  impl ZshHistoryParser {
      pub fn new() -> Self;
      /// Complete commands found in `bytes`, oldest first. An entry is complete at a LF that
      /// isn't preceded by `\`; a trailing `\`+LF joins the next line with "\n" (zsh's hist.c).
      /// The `: <digits>:<digits>;` EXTENDED_HISTORY header is stripped after unmetafying.
      /// Empty commands and entries that aren't valid UTF-8 are skipped (never suggested).
      pub fn feed(&mut self, bytes: &[u8]) -> Vec<String>;
  }
  ```

- [ ] **Step 1: Generate the fixture with real zsh** (it's the only honest source of metafied bytes;
  the core itself never runs zsh):

```bash
zsh -f -i -c 'HISTFILE=$1; HISTSIZE=10; SAVEHIST=10; setopt extendedhistory
print -s "echo 日本語"; print -s "for i in 1 2
do
  echo \$i
done"; print -s "git status"; fc -W' _ core/tests/fixtures/zsh_history_extended </dev/null 2>/dev/null
xxd core/tests/fixtures/zsh_history_extended | head -3
```
Expected: line 1 shows `: <10 digits>:0;echo ` then `e683 b7a5` (日 = `e6 97 a5`, metafied). Checked
on zsh 5.9 on 2026-11-13. Don't use `-i` without `</dev/null`.

- [ ] **Step 2: Write the failing tests** (unit tests in `zsh_history.rs`, fixture test in `core/tests/zsh_history_fixture.rs`):

```rust
#[test] fn plain_lines_are_commands() { assert_eq!(feed_all(b"ls\ngit status\n"), ["ls", "git status"]); }
#[test] fn extended_header_is_stripped() { assert_eq!(feed_all(b": 1794524352:0;git push\n"), ["git push"]); }
#[test] fn header_lookalike_without_digits_is_kept() { assert_eq!(feed_all(b": x;y\n"), [": x;y"]); }
#[test] fn backslash_newline_joins_multiline_command() {
    assert_eq!(feed_all(b"for i in 1 2\\\ndo echo $i\\\ndone\n"), ["for i in 1 2\ndo echo $i\ndone"]);
}
#[test] fn metafied_bytes_are_decoded() { assert_eq!(feed_all(b"echo \xe6\x83\xb7\xa5\n"), ["echo 日"]); }
#[test] fn unterminated_last_line_waits_for_more() {
    let mut p = ZshHistoryParser::new();
    assert_eq!(p.feed(b"ls\ngit st"), ["ls"]);
    assert_eq!(p.feed(b"atus\n"), ["git status"]);
}
#[test] fn continuation_split_across_feeds() { /* feed(b"a\\") → [], feed(b"\nb\n") → ["a\nb"] */ }
#[test] fn invalid_utf8_entry_is_skipped_others_kept() { assert_eq!(feed_all(b"ok1\n\xff\xfe\nok2\n"), ["ok1", "ok2"]); }
#[test] fn empty_lines_are_skipped() { assert_eq!(feed_all(b"\n\nls\n"), ["ls"]); }
// core/tests/zsh_history_fixture.rs
#[test] fn fixture_written_by_real_zsh_parses() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/zsh_history_extended")).unwrap();
    assert_eq!(ZshHistoryParser::new().feed(&bytes), ["echo 日本語", "for i in 1 2\ndo\n  echo $i\ndone", "git status"]);
}
```

- [ ] **Step 3: Run** `cargo test -p fuxx-completes-core zsh_history` — Expected: FAIL.
- [ ] **Step 4: Implement** per Interfaces. Order per entry: join continuation lines (raw bytes), then
  `unmetafy`, then strip the header, then UTF-8 check.
- [ ] **Step 5: Run** `cargo test -p fuxx-completes-core` — Expected: PASS.
- [ ] **Step 6: Docs.** Roadmap Progress log line "zsh history parser". No design change.
- [ ] **Step 7: Commit:** `feat(core): zsh history parser (metafied bytes, EXTENDED_HISTORY, multi-line)`.

---

### Task 3: History index and the suggestion

**Files:**
- Create: `core/src/history.rs`
- Modify: `core/src/lib.rs` (`pub mod history;`), `fuxx-autosuggestions/Cargo.toml` (path dep on `fuxx-completes-core`), `fuxx-autosuggestions/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  ```rust
  // core/src/history.rs
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum HistoryFormat { Zsh }
  impl std::str::FromStr for HistoryFormat { type Err = (); }   // "zsh" only in M0
  #[derive(Debug, Default)] pub struct History { /* file: Vec<String>, session: Vec<String> */ }
  impl History {
      pub fn new() -> Self;
      /// Most recent entry starting with `prefix`: session records newest-first, then file entries
      /// newest-first. Plain byte-prefix match, case-sensitive.
      pub fn latest_with_prefix(&self, prefix: &str) -> Option<&str>;
      pub fn record_session(&mut self, command: String);
      /// New entries read from the end of the file; each one removes one equal session record.
      pub fn append_file_entries(&mut self, entries: Vec<String>);
      /// A full re-read: replaces the file entries; each new entry removes one equal session record.
      pub fn replace_file_entries(&mut self, entries: Vec<String>);
  }
  // fuxx-autosuggestions/src/lib.rs
  /// The text to show after `buffer`, or None. None for an empty buffer, no match, or an exact match.
  pub fn suggestion<'a>(history: &'a History, buffer: &str) -> Option<&'a str>;
  ```

- [ ] **Step 1: Write the failing tests:**

```rust
// core/src/history.rs
#[test] fn most_recent_file_match_wins() {
    let mut h = History::new(); h.replace_file_entries(vec!["git push a".into(), "git pull".into(), "git push b".into()]);
    assert_eq!(h.latest_with_prefix("git pu"), Some("git push b"));
}
#[test] fn session_records_rank_above_file_entries() { /* file ["git pull"], session "git push" → "git push" */ }
#[test] fn no_match_is_none() { /* "zz" → None */ }
#[test] fn appended_entry_drops_one_matching_session_record() {
    // session ["make"], append ["ls","make"] → session empty; latest_with_prefix("ma") == Some("make") from the file
}
#[test] fn full_reread_keeps_unwritten_session_records() {
    // session ["a1","b1"], replace_file_entries(["a1"]) → "b" still finds "b1", "a" finds "a1"
}
#[test] fn match_is_case_sensitive() { /* file ["Git x"], prefix "git" → None */ }
// fuxx-autosuggestions/src/lib.rs
#[test] fn returns_text_after_buffer() { /* file ["git push origin main"], "git p" → Some("ush origin main") */ }
#[test] fn empty_buffer_gives_none() {}
#[test] fn exact_match_gives_none() { /* most recent match equals buffer → None (zsh-autosuggestions parity) */ }
#[test] fn multibyte_prefix_splits_cleanly() { /* file ["echo 日本語"], "echo 日" → Some("本語") */ }
#[test] fn multiline_entry_suggests_rest() { /* file ["for i\ndo x\ndone"], "for" → Some(" i\ndo x\ndone") */ }
```

- [ ] **Step 2: Run** `cargo test -p fuxx-completes-core history -p fuxx-autosuggestions` — Expected: FAIL.
- [ ] **Step 3: Implement** per Interfaces (linear scans are fine; 100k strings is well under 10 ms).
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Docs.** Progress log line. `docs/design.md` §5.8 "Session records" bullet: add "ranked
  above file entries until the file holds them; each entry the core reads from the file retires one
  equal session record (AI's call, M0)".
- [ ] **Step 6: Commit:** `feat(core): history index; feat(autosuggestions): suggestion()`.

---

### Task 4: Re-reading the history file

**Files:**
- Create: `core/src/history_file.rs`
- Modify: `core/src/lib.rs` (`pub mod history_file;`), `core/Cargo.toml` (`[dev-dependencies] tempfile = "3.27"`)

**Interfaces:**
- Consumes: `ZshHistoryParser`, `History`, `HistoryFormat`.
- Produces:
  ```rust
  pub struct HistoryFile { /* path, format, (dev, ino, len, mtime) of what was read, parser, reload: Option<JoinHandle<..>> */ }
  impl HistoryFile {
      /// Starts a background full read at once; never blocks.
      pub fn new(path: PathBuf, format: HistoryFormat) -> Self;
      /// Never blocks on a full read. Applies a finished background read (replace_file_entries);
      /// then stats the file: same dev+inode and longer → reads only the new bytes inline
      /// (append_file_entries); any other change (new inode, shorter, same length but new mtime)
      /// → starts a background full read unless one is running. A missing file is "no entries".
      pub fn refresh(&mut self, history: &mut History);
      pub fn is_reloading(&self) -> bool;
  }
  ```
  Use `std::os::unix::fs::MetadataExt` for dev/ino, `std::thread::spawn` for the full read, and
  `JoinHandle::is_finished` to poll. The incremental read keeps the parser's pending bytes.

- [ ] **Step 1: Write the failing tests** (helper `settle(&mut f, &mut h)` = loop `refresh` + 10 ms sleep until `!is_reloading()`, panic after 2 s):

```rust
#[test] fn first_read_happens_in_background() { /* write "ls\n"; new(); settle; latest_with_prefix("l") == Some("ls") */ }
#[test] fn missing_file_gives_no_entries_and_no_panic() { /* path in tempdir that doesn't exist; settle; None */ }
#[test] fn appended_lines_are_seen_without_waiting() {
    // settle on "ls\n"; append "git status\n" with OpenOptions::append; ONE refresh (no settle) →
    // latest_with_prefix("git") == Some("git status")
}
#[test] fn partial_line_is_completed_by_next_append() { /* append "git st" → None for "git"; append "atus\n" → Some */ }
#[test] fn rewritten_file_is_reloaded() {
    // settle on "secret\nls\n"; write "ls\n" to a sibling and rename over (new inode); settle →
    // latest_with_prefix("sec") == None   (deletions are respected)
}
#[test] fn truncated_file_is_reloaded() { /* same inode, File::create + write shorter content → old entry gone */ }
#[test] fn large_history_loads_off_the_request_path() {
    // 100_000 lines "cmd {i}\n"; new() then refresh() returns while is_reloading() is true
    // (assert refresh took < 5 ms with std::time::Instant); after settle, "cmd 99999" is found
}
```

- [ ] **Step 2: Run** `cargo test -p fuxx-completes-core history_file` — Expected: FAIL.
- [ ] **Step 3: Implement** per Interfaces.
- [ ] **Step 4: Run** `cargo test --workspace` — Expected: PASS.
- [ ] **Step 5: Docs.** Progress log line. §5.8 "Re-reading" bullet: add "a growth is trusted only on the
  same device and inode; anything else is a full re-read (AI's call, M0)".
- [ ] **Step 6: Commit:** `feat(core): re-read the history file on change, full reads in the background`.

---

### Task 5: The `fuxx-completes` binary (`cli/` crate)

**Files:**
- Create: `cli/Cargo.toml` (package `fuxx-completes`, `[[bin]] name = "fuxx-completes"`, deps: `fuxx-completes-core`, `fuxx-autosuggestions` by path; dev-deps `expectrl = "0.9"`, `tempfile = "3.27"`), `cli/src/lib.rs` (`pub mod session; pub mod serve;`), `cli/src/session.rs`, `cli/src/serve.rs`, `cli/src/main.rs`, `cli/tests/serve.rs`
- Modify: root `Cargo.toml` (`members` += `"cli"`), `docs/design.md` §5.5, §12

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces:
  ```rust
  // cli/src/session.rs
  pub struct Session { /* history: History, file: Option<HistoryFile> */ }
  impl Session {
      pub fn new() -> Self;
      /// Hello → set up the HistoryFile (zsh format, non-empty path) and reply Ready{PROTOCOL_VERSION}
      /// whatever version was asked (the adapter checks it). Suggest → refresh, then
      /// Suggestion{id, fuxx_autosuggestions::suggestion(..)}. Ran → record_session(command), no reply.
      /// Bye → no reply.
      pub fn handle(&mut self, req: Request) -> Option<Response>;
      /// DecodeError::Malformed{id: Some(id)} → Suggestion{id, None}; anything else → None.
      pub fn handle_error(&mut self, err: DecodeError) -> Option<Response>;
  }
  // cli/src/serve.rs
  /// Reads LF-terminated lines until EOF or `bye`; writes and flushes each reply at once.
  pub fn serve(input: impl BufRead, output: impl Write) -> std::io::Result<()>;
  ```
  `main.rs`: `fuxx-completes serve` → `serve(stdin.lock(), stdout.lock())`, exit 0 on EOF; any other
  argument list → usage on stderr, exit 2. No other subcommands in M0 (`report`, `forget`, `doctor`
  are M1/M2).

- [ ] **Step 1: Write the failing tests:**

```rust
// cli/src/session.rs
#[test] fn hello_replies_ready_1() {}
#[test] fn suggest_before_hello_answers_none() { /* Suggestion{id:1,text:None} */ }
#[test] fn ran_command_is_suggested() { /* hello with empty path; Ran{command:"make test"}; Suggest "make" → Some(" test") */ }
#[test] fn malformed_suggest_gets_empty_reply() { /* handle_error(Malformed{id:Some(5)}) == Some(Suggestion{id:5,text:None}) */ }
#[test] fn unknown_kind_gets_no_reply() {}
// cli/src/serve.rs
#[test] fn serves_lines_until_eof() { /* Cursor input: hello + suggest; output == ready line + suggestion line */ }
#[test] fn bye_stops_reading() { /* input: bye + suggest → no suggestion line */ }
#[test] fn garbage_line_is_skipped_and_serving_continues() { /* "\xff\xff\n" then suggest → one suggestion line */ }
// cli/tests/serve.rs — the real binary, env!("CARGO_BIN_EXE_fuxx-completes"), piped stdin/stdout, temp HOME
#[test] fn binary_answers_from_a_zsh_history_file() {
    // history file "git status\ngit push origin main\n"; send hello(path) then, after reading "ready",
    // retry suggest "git p" (ids 1, 2, …, every 20 ms, max 2 s) until text == "ush origin main"
}
#[test] fn binary_echoes_ids() { /* suggest ids 41 and 42 → replies carry 41 then 42 */ }
#[test] fn binary_exits_zero_on_eof() {}
#[test] fn binary_without_serve_exits_2() {}
#[test] fn binary_writes_nothing_to_stderr_while_serving() {}
```

- [ ] **Step 2: Run** `cargo test -p fuxx-completes` — Expected: FAIL.
- [ ] **Step 3: Implement** per Interfaces.
- [ ] **Step 4: Run** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` — Expected: PASS, clean.
- [ ] **Step 5: Docs.** §5.5 last paragraph: "The binary lives in `cli/` (package and binary
  `fuxx-completes`), depending on the core and the libraries (**AI's call, settled in the M0 plan**)."
  §12 layout: add `cli/  the fuxx-completes binary (serve; later report, complete, forget, doctor)`.
  Progress log line.
- [ ] **Step 6: Commit:** `feat(cli): fuxx-completes serve`.

---

### Task 6: zsh adapter — core process, ghost text, async

**Files:**
- Create: `adapters/zsh/fuxx-autosuggestions.plugin.zsh`, `cli/tests/common/mod.rs`, `cli/tests/zsh_adapter.rs`
- Modify: `docs/design.md` §5.5 (transport bullet), §5.6

**Interfaces:**
- Consumes: the binary (Task 5) and wire format 1.
- Produces (shell, used by Tasks 7–8; all globals `typeset -g`, all names prefixed so nothing clashes):
  - Config (read at use, never required): `FUXX_COMPLETES_BIN`, `FUXX_AUTOSUGGESTIONS_HIGHLIGHT_STYLE` (default `fg=8`).
  - State: `_FUXX_AS_STATE` (`''` until the first prompt, `on`, `off`), `_FUXX_AS_PID`, `_FUXX_AS_WFD`,
    `_FUXX_AS_RFD`, `_FUXX_AS_KEEPFD`, `_FUXX_AS_NEXT_ID`, `_FUXX_AS_INFLIGHT_ID`,
    `_FUXX_AS_INFLIGHT_SINCE` (`$EPOCHREALTIME`), `_FUXX_AS_INFLIGHT_BUFFER`, `_FUXX_AS_LAST_BUFFER`,
    `_FUXX_AS_HL` (exact highlight string we added).
  - Functions: `_fuxx_as_start`, `_fuxx_as_stop`, `_fuxx_as_send <kind> <fields…>`,
    `_fuxx_as_request` (suggest for `$BUFFER`), `_fuxx_as_on_response <fd> [err]`,
    `_fuxx_as_show <text>`, `_fuxx_as_clear`, `_fuxx_as_wrap_widgets`, `_fuxx_as_w_modify`, and the
    widget `fuxx-as-show` (calls `_fuxx_as_show "$1"` then `zle -R`).
  - Original widgets are kept as `fuxx-as-orig-<name>` (`zle -A <name> fuxx-as-orig-<name>`); a
    wrapper finds its original through `$WIDGET`.
- Test harness (`cli/tests/common/mod.rs`), used by Tasks 6–9:
  ```rust
  pub struct ZshOpts { pub history: Vec<&'static str>, pub rc_before: String, pub rc_after: String,
                       pub core_bin: Option<PathBuf> /* default: the cargo-built binary */ }
  #[derive(Default)] // ZshOpts too
  pub struct LineState { pub buffer: String, pub cursor: usize, pub postdisplay: String,
                         pub region_highlight: Vec<String>, pub core_pid: Option<u32> /* $_FUXX_AS_PID */ }
  pub struct ZshHarness { /* expectrl::Session, tempfile::TempDir HOME */ }
  impl ZshHarness {
      pub fn start(opts: ZshOpts) -> Self;          // waits for the "READY> " prompt, 5 s max
      pub fn send(&mut self, keys: &str);
      pub fn run(&mut self, line: &str);            // sends line + "\r", waits for the next prompt
      pub fn wait_state(&mut self, pred: impl Fn(&LineState) -> bool) -> LineState; // 3 s, panics with last state
      pub fn state(&mut self) -> LineState;
      pub fn screen_since_start(&self) -> String;   // all pty output so far
      pub fn home(&self) -> &Path;  pub fn histfile(&self) -> PathBuf;
  }
  pub fn zsh_path() -> String { std::env::var("FUXX_TEST_ZSH").unwrap_or("zsh".into()) }
  pub fn zsh_at_least(v: &str) -> bool;            // runs `$zsh -fc 'autoload is-at-least; is-at-least v'`
  ```
  `start` writes `$HOME/.zshrc` = `rc_before` + `HISTFILE=$HOME/.zsh_history; HISTSIZE=1000;
  SAVEHIST=1000; PS1='READY> '; bindkey -e; KEYTIMEOUT=1` + `source <repo>/adapters/zsh/fuxx-autosuggestions.plugin.zsh`
  + a dump widget `_fuxx-test-dump` bound to `^[[9999~` that writes
  `BUFFER US CURSOR US POSTDISPLAY US ${(pj:\x1e:)region_highlight} US $_FUXX_AS_PID` to `$HOME/dump` + `rc_after`.
  `history` entries are written as zsh writes them (plain format, a newline inside an entry as `\`+LF).
  It spawns `zsh_path() -d -i` in `current_dir($HOME)` with `env_clear()` and only `HOME`, `ZDOTDIR=$HOME`, `XDG_*` under
  `$HOME`, `PATH=/usr/bin:/bin`, `TERM=xterm-256color`, `LANG=en_US.UTF-8`, `FUXX_COMPLETES_BIN`.
  `state()` deletes `dump`, sends `\x1b[9999~`, polls for the file. Stub cores are tiny `/bin/sh`
  scripts the test writes into `$HOME`: recorder `exec cat > "$HOME/wire.log"`, silent
  `echo $$ > "$HOME/core.pid"; exec cat > /dev/null`, dead `exit 0`.

- [ ] **Step 1: Check the dev crates through Context7** (`expectrl`: `Session::spawn(Command)`,
  `send`, `expect`, `set_expect_timeout`; `tempfile::TempDir`). Note any API difference in the task report.
- [ ] **Step 2: Write the failing tests** in `cli/tests/zsh_adapter.rs`:

```rust
#[test] fn ghost_text_appears_for_a_history_prefix() {
    let mut z = ZshHarness::start(ZshOpts { history: vec!["git status", "git push origin main"], ..Default::default() });
    z.send("git p");
    let s = z.wait_state(|s| s.postdisplay == "ush origin main");
    assert_eq!(s.buffer, "git p");
}
#[test] fn ghost_text_is_dimmed_and_tagged() {
    // after the above: region_highlight contains exactly one of ours:
    // zsh ≥ 5.9: "5 20 fg=8 memo=fuxx-autosuggestions"; below: "5 20 fg=8"
}
#[test] fn highlight_style_is_configurable() { /* rc_before "FUXX_AUTOSUGGESTIONS_HIGHLIGHT_STYLE=fg=red" → entry contains "fg=red" */ }
#[test] fn typing_a_non_match_clears_ghost_text() { /* "git p" then "x" → postdisplay "" and no entry of ours left */ }
#[test] fn foreign_region_highlight_entry_survives() {
    // rc_after: a line-init hook (add-zle-hook-widget) adds "0 1 bold"; "git p" then "x" → "0 1 bold" still present
}
#[test] fn prints_nothing_at_startup() { /* screen_since_start() before typing == only the prompt text (no other printable text) */ }
#[test] fn sends_hello_then_one_suggest_in_flight() {
    // core_bin = recorder; send "abc"; then wire.log (poll ≤ 3 s) has exactly:
    // "hello\x1f1\x1fzsh\x1f<ZSH_VERSION>\x1fzsh\x1f<HOME>/.zsh_history\x1fautosuggestions"
    // and ONE "suggest\x1f1\x1fasync\x1f1\x1f<HOME>\x1fa" line (the other keys wait for the answer)
}
#[test] fn missing_binary_stays_silent() { /* core_bin = "/nonexistent"; run("echo hi") prints "hi"; screen has no "fuxx" */ }
#[test] fn core_killed_mid_session_goes_quiet() {
    // real core; wait for a suggestion; `kill -9 <state().core_pid>`; run("echo hi") prints "hi";
    // no "fuxx" and no zsh error text on screen; a later keystroke gives no ghost text
}
#[test] fn slow_core_never_blocks_typing() { /* silent stub; run("echo typed") shows "typed" within 1 s */ }
#[test] fn hung_core_is_stopped_after_two_seconds() {
    // silent stub; send "e"; sleep 2.5 s; send "c"; then `kill -0 <core.pid>` fails within 1 s
}
#[test] fn adapter_survives_hostile_user_options() {
    // rc_before "setopt ksharrays shwordsplit nounset warncreateglobal"; ghost text appears; screen has no error text
}
#[test] fn multiline_buffer_gets_suggestion() {
    // history ["for i in 1 2\ndo\n  echo $i\ndone"]; send "for i in 1 2" + "\x1b\r" (self-insert-unmeta newline)
    // → postdisplay == "do\n  echo $i\ndone"
}
#[test] fn separator_byte_in_buffer_sends_nothing() {
    // recorder; FIRST keys "\x16\x1f" (⌃V then 0x1F) then "a" — the buffer always holds 0x1F, so after
    // 1 s wire.log has the hello line and no "suggest" line; then "\x15" (⌃U) and run("echo ok") prints "ok"
}
```

- [ ] **Step 3: Run** `cargo test -p fuxx-completes --test zsh_adapter` — Expected: FAIL (no plugin file).
- [ ] **Step 4: Implement the plugin's load and start.** At source time: return quietly unless
  interactive and (`autoload -Uz is-at-least add-zsh-hook add-zle-hook-widget`) `is-at-least 5.3`;
  `zmodload zsh/datetime zsh/parameter`;
  `zmodload -F zsh/files b:zf_rm b:zf_rmdir b:zf_mkdir`; define functions; `add-zsh-hook precmd
  _fuxx_as_precmd`; `add-zsh-hook zshexit _fuxx_as_zshexit` (sends `bye`); nothing else runs, nothing
  prints. `_fuxx_as_precmd`, first call only (`_FUXX_AS_STATE` empty) → `_fuxx_as_start`:
  1. Binary: `${FUXX_COMPLETES_BIN:-${commands[fuxx-completes]}}`; not executable → state `off`, return.
  2. `zf_mkdir -m 700 "${TMPDIR:-/tmp}/fuxx-completes.$$.$RANDOM"`; `command mkfifo -m 600 $dir/req $dir/resp`.
  3. The open order (verified on macOS 2026-11-13 — no step blocks, a dead core gives EOF on the
     read fd, and writes never raise `EPIPE` because the shell keeps a reader on `req`):
     ```zsh
     exec {_FUXX_AS_KEEPFD}<>$dir/req {rs}<>$dir/resp
     "$bin" serve <$dir/req >$dir/resp 2>/dev/null &!
     _FUXX_AS_PID=$!
     exec {_FUXX_AS_WFD}>$dir/req {_FUXX_AS_RFD}<$dir/resp
     exec {rs}>&-
     zf_rm -f $dir/req $dir/resp; zf_rmdir $dir
     ```
     Any failure → `_fuxx_as_stop`, state `off`. Every command's stderr → `/dev/null`.
  4. `zle -F $_FUXX_AS_RFD _fuxx_as_on_response` (a plain function, not `-w`, so it gets the error
     argument; it calls `zle fuxx-as-show -- "$text"` to draw, as zsh-autosuggestions does).
  5. `_fuxx_as_send hello 1 zsh $ZSH_VERSION zsh ${HISTFILE:+${HISTFILE:a}} autosuggestions`;
     `_fuxx_as_wrap_widgets`; state `on`.
- [ ] **Step 5: Implement requests and answers.**
  - `_fuxx_as_send`: join fields with `$'\x1f'`, each field's `$'\n'` → `$'\x1e'`, `print -rn -u $_FUXX_AS_WFD -- "$line"$'\n'`.
  - `_fuxx_as_request`: nothing if state isn't `on`, `$BUFFER` is empty, `$#BUFFER > 1000`, or
    `$BUFFER` contains `$'\x1f'` or `$'\x1e'`. If a request is in flight: if it is older than 2 s
    (`EPOCHREALTIME`) → `_fuxx_as_stop` (also `kill $_FUXX_AS_PID`); else return (the answer
    handler re-sends). Otherwise send `suggest $id async $CURSOR $PWD $BUFFER` and record id, time, buffer.
  - `_fuxx_as_on_response fd err`: `err` non-empty, or `IFS= read -r -u $fd line` fails → `_fuxx_as_stop`.
    Split with `"${(@ps:\x1f:)line}"` (keeps empty fields), map `\x1e` → `\n`. `ready` with a
    protocol other than `1` → stop. `suggestion` with id ≠ in-flight id → drop. Matching id → clear
    in-flight; if `$BUFFER` ≠ the in-flight buffer → `_fuxx_as_request` again and draw nothing; else
    `zle fuxx-as-show -- "$text"`.
  - `_fuxx_as_stop`: `zle -F $_FUXX_AS_RFD` (if set), close the three fds, `kill` the pid
    (`2>/dev/null`), state `off`, `_fuxx_as_clear`. Silent.
- [ ] **Step 6: Implement drawing and wrapping.**
  - `_fuxx_as_show text`: `_fuxx_as_clear`; `POSTDISPLAY=$text`; if non-empty, entry =
    `"$#BUFFER $(( $#BUFFER + $#POSTDISPLAY )) ${FUXX_AUTOSUGGESTIONS_HIGHLIGHT_STYLE:-fg=8}"`, plus
    `" memo=fuxx-autosuggestions"` when `is-at-least 5.9` (check once at load); append it to
    `region_highlight`, remember it in `_FUXX_AS_HL`.
  - `_fuxx_as_clear`: `POSTDISPLAY=`; remove ours: 5.9+ `region_highlight=( ${region_highlight:#*memo=fuxx-autosuggestions} )`;
    below `region_highlight=( ${region_highlight:#${(b)_FUXX_AS_HL}} )`; `_FUXX_AS_HL=`.
  - `_fuxx_as_wrap_widgets` (first precmd, after the user's rc): for each name in `zle -la` except
    names starting `.`, `_`, `fuxx-as-`, `orig-`, `zle-`, and `beep run-help set-local-history
    which-command yank yank-pop`: `zle -A $w fuxx-as-orig-$w`, then `zle -N $w <wrapper>`, the
    wrapper picked from the lists in Task 7 or `_fuxx_as_w_modify`. Use `add-zle-hook-widget
    line-init` (reset `_FUXX_AS_LAST_BUFFER=`) and `line-finish` (`_fuxx_as_clear`, so ghost text
    never stays in the scrollback).
  - `_fuxx_as_w_modify`: run `zle fuxx-as-orig-$WIDGET -- "$@"`, keep its status; if `$BUFFER` ≠
    `_FUXX_AS_LAST_BUFFER`: `_fuxx_as_clear`, `_FUXX_AS_LAST_BUFFER=$BUFFER`, `_fuxx_as_request`;
    return the status.
- [ ] **Step 7: Run** `cargo test -p fuxx-completes --test zsh_adapter` — Expected: PASS on `/bin/zsh` (5.9).
- [ ] **Step 8: Docs.** §5.5 transport bullet: replace "The builder picks one in M0" with the named-pipe
  decision, the open order in one sentence, one request in flight with the 2 s hang rule, the 1000
  character cap, and the binary lookup (`FUXX_COMPLETES_BIN`, then `PATH`) — each marked **AI's call,
  M0**. §5.6: add "In M0 the zsh adapter is one self-contained file,
  `adapters/zsh/fuxx-autosuggestions.plugin.zsh`, that finds everything through `PATH` or variables,
  never through its own location" (keeps the §8 zip layout and M1's `FUXX_PLUGIN_DATA` easy). Progress log line.
- [ ] **Step 9: Commit:** `feat(zsh): adapter starts serve on named pipes and draws ghost text asynchronously`.

---

### Task 7: Accept keys, partial accept, clear widgets, End and ⌥→, vi mode

**Files:**
- Modify: `adapters/zsh/fuxx-autosuggestions.plugin.zsh`, `cli/tests/zsh_adapter.rs`, `docs/design.md` §6

**Interfaces:**
- Consumes: Task 6's state, `_fuxx_as_show`, `_fuxx_as_clear`, `_fuxx_as_wrap_widgets`.
- Produces: `_fuxx_as_w_accept`, `_fuxx_as_w_partial`, `_fuxx_as_w_clear`, `_fuxx_as_bind_if_free <keymap> <seq> <widget>`, and the lists:
  - accept: `forward-char end-of-line vi-forward-char vi-end-of-line vi-add-eol`
  - partial: `forward-word emacs-forward-word vi-forward-word vi-forward-word-end vi-forward-blank-word vi-forward-blank-word-end vi-find-next-char vi-find-next-char-skip`
  - clear: `history-search-forward history-search-backward history-beginning-search-forward
    history-beginning-search-backward history-beginning-search-forward-end
    history-beginning-search-backward-end history-substring-search-up history-substring-search-down
    up-line-or-beginning-search down-line-or-beginning-search up-line-or-history down-line-or-history
    accept-line copy-earlier-word` (zsh-autosuggestions' defaults, checked through Context7 2026-11-13)

- [ ] **Step 1: Write the failing tests** (history `["git status", "git push origin main"]`, then `send("git p")` and wait for the ghost text first):

```rust
#[test] fn right_arrow_accepts_whole_suggestion() { /* "\x1b[C" → buffer "git push origin main", postdisplay "", cursor 20 */ }
#[test] fn ctrl_e_accepts_whole_suggestion() { /* "\x05" → same */ }
#[test] fn esc_f_accepts_one_word() { /* "\x1bf" → buffer "git push", postdisplay " origin main", one entry of ours spanning 8..20 */ }
#[test] fn forward_char_mid_buffer_only_moves() { /* "\x02" (⌃B) then "\x1b[C" → buffer "git p", cursor 5, postdisplay "ush origin main" */ }
#[test] fn end_key_accepts_when_free() { /* "\x1b[F" and, in a fresh harness, "\x1b[4~" → whole suggestion */ }
#[test] fn alt_right_accepts_a_word_when_free() { /* "\x1b[1;3C" → buffer "git push" */ }
#[test] fn user_binding_of_end_is_kept() {
    // rc_before "bindkey '^[[F' beginning-of-line"; "\x1b[F" → cursor 0, buffer "git p", postdisplay still shown
}
#[test] fn user_binding_of_alt_right_is_kept() { /* rc_before binds '^[[1;3C' to beginning-of-line; same check */ }
#[test] fn accept_line_clears_ghost_text() { /* "\r" runs "git p" (command not found is fine); screen line for it has no "ush origin" */ }
#[test] fn up_arrow_clears_ghost_text() { /* "\x1b[A" → postdisplay "" */ }
#[test] fn vi_end_of_line_accepts() { /* rc_before "bindkey -v"; "git p", "\x1b", "$" → buffer "git push origin main" */ }
#[test] fn vi_add_eol_accepts() { /* "\x1b", "A" → same */ }
#[test] fn vi_forward_word_accepts_part() { /* "\x1b", "w" → buffer "git push o", postdisplay "rigin main" (zsh-autosuggestions parity) */ }
#[test] fn tab_ctrl_r_ctrl_t_are_not_bound_by_us() {
    // compare `bindkey '^I' '^R' '^T' '^[[A'` output inside the harness with and without the plugin (rc_after dumps it to a file)
}
```

- [ ] **Step 2: Run** `cargo test -p fuxx-completes --test zsh_adapter` — Expected: the new tests FAIL.
- [ ] **Step 3: Implement.**
  - `_fuxx_as_w_accept`: max = `$#BUFFER`, minus 1 in `vicmd`; if `$POSTDISPLAY` is empty or
    `$CURSOR` ≠ max → just run the original. Else `BUFFER+=$POSTDISPLAY`, `_fuxx_as_clear`, run the
    original, then `CURSOR=$#BUFFER` (`$#BUFFER - 1` in `vicmd`); `_FUXX_AS_LAST_BUFFER=$BUFFER`.
  - `_fuxx_as_w_partial`: save `$BUFFER`; `BUFFER+=$POSTDISPLAY`; run the original; `loc=$CURSOR`
    (+1 in `vicmd`); if `loc > $#saved` → `POSTDISPLAY=${BUFFER[loc+1,-1]}`, `BUFFER=${BUFFER[1,loc]}`,
    redraw our highlight for the new offsets; else restore `BUFFER=saved`. `_FUXX_AS_LAST_BUFFER=$BUFFER`
    so no request is sent (the rest of the suggestion is still right).
  - `_fuxx_as_w_clear`: `_fuxx_as_clear`, run the original, `_FUXX_AS_LAST_BUFFER=$BUFFER`.
  - `_fuxx_as_bind_if_free km seq w`: bind only if `bindkey -M $km $seq` ends in `undefined-key`.
    In `_fuxx_as_start`, after wrapping, for km in `emacs viins`: `^[[F` and `^[[4~` → `end-of-line`,
    `^[[1;3C` → `forward-word`.
- [ ] **Step 4: Run** the adapter tests — Expected: all PASS on 5.9.
- [ ] **Step 5: Docs.** §6 Keys bullet: add "in the `emacs` and `viins` keymaps; `vicmd` is left
  alone (AI's call, M0)". Progress log line.
- [ ] **Step 6: Commit:** `feat(zsh): accept, partial accept and clear widgets; End and ⌥→ only if free; vi mode`.

---

### Task 8: Command reports (§5.8) and the re-read, end to end

**Files:**
- Modify: `adapters/zsh/fuxx-autosuggestions.plugin.zsh`, `cli/tests/zsh_adapter.rs`, `docs/design.md` §5.8

**Interfaces:**
- Consumes: `_fuxx_as_send`, `_fuxx_as_precmd` (Task 6).
- Produces: `_fuxx_as_addhistory` (zshaddhistory hook, notes `${1%$'\n'}` in `_FUXX_AS_NOTED`,
  **always returns 0**), `_fuxx_as_preexec` (saves `$PWD` in `_FUXX_AS_CMD_PWD`), the pending
  command `_FUXX_AS_PEND_EVENT`, `_FUXX_AS_PEND_ENTRY`, `_FUXX_AS_PEND_PWD`, `_FUXX_AS_PEND_STATUS`,
  `_FUXX_AS_PEND_TIME`, and the report step at the start of every `_fuxx_as_precmd`.

**One prompt later (the user's decision, 2026-11-13, option b).** zsh keeps a line it won't save
(`HIST_IGNORE_SPACE`, `HIST_NO_STORE`, a `zshaddhistory` hook returning 1, an alias with a leading
space) in `$history` until the next command is entered, then drops it, and the next command takes
over its event number (zsh docs; checked on 5.9 on 2026-11-13 with a stdin-fed `zsh -i`: a
leading-space line, a rejecting hook and a leading-space alias each had their event number reused;
re-checked on 5.3.1/5.8.1 by Task 9). So the adapter holds each command back as *pending* and
decides at the next prompt where a new line went into history. Cost, accepted by the user: the
command just run isn't suggested until another command runs, and a session's last command is never
reported (memory only; the history file still has it).

**The check.** At `precmd`, with `status=$?` and the user's `histignorespace`/`histnostore` read
from `$options` *before* `emulate -L zsh`. Nothing happens unless `_FUXX_AS_NOTED` is non-empty (an
empty line changes nothing; the pending command keeps waiting). Then, with `n=$((HISTCMD-1))` (the
newest event):
1. **Decide the pending command** (event `p`, if any): it was kept only if `n > p` **and**
   `${history[$p]}` equals `_FUXX_AS_PEND_ENTRY`. Kept → send `ran $_FUXX_AS_PEND_PWD
   $_FUXX_AS_PEND_STATUS $_FUXX_AS_PEND_TIME $_FUXX_AS_PEND_ENTRY`. Kept or not, clear the pending
   command. (`n == p` means the new command took over a dropped line's number; when in doubt, don't
   report — a `HIST_IGNORE_DUPS` repeat is then reported one command later still, never lost text.)
2. **Hold the new command back:** `E=${history[$n]}`. It becomes pending (event `n`, `E`,
   `$_FUXX_AS_CMD_PWD`, `$status`, `$EPOCHSECONDS`) only if: `E` is non-empty; not
   (`histignorespace` and the noted line starts with a space); not (`histnostore` and `E` is
   `history…` or `fc -l…`); not (`HISTORY_IGNORE` set and `[[ $E == ${~HISTORY_IGNORE} ]]` —
   `HISTORY_IGNORE` only filters the file, so it must be checked here); `$#E` ≤ 1000. (The first two
   are also caught by step 1; they stay as a cheap second guard.)
3. Clear `_FUXX_AS_NOTED`.
Report `E`, not the raw line, so `HIST_REDUCE_BLANKS` matches the file.

- [ ] **Step 1: Write the failing tests** (every test that expects a command to be suggested runs one
  more command after it, `run("true")`, because reports are one prompt late):

```rust
#[test] fn command_run_in_session_is_suggested() { /* empty history; run("echo one"); run("true"); send("echo o") → postdisplay "ne" */ }
#[test] fn report_waits_for_the_next_command() {
    // recorder stub; run("echo one"); after 1 s wire.log has no "ran" line; run("true") → wire.log
    // gets one ran line ending "\x1fecho one" and none ending "\x1ftrue"
}
#[test] fn space_led_command_is_not_suggested() {
    // rc_before "setopt histignorespace"; run("echo shown"); run(" echo secret"); run("true"); send("echo s") → postdisplay "hown"
}
#[test] fn history_ignore_match_is_not_suggested() {
    // rc_before "HISTORY_IGNORE='echo secret*'"; run("echo shown"); run("echo secret1"); run("true"); send("echo s") → "hown"
}
#[test] fn line_rejected_by_another_zshaddhistory_hook_is_not_suggested() {
    // rc_after "_t_rej() { [[ $1 != 'echo secret'* ]] }; add-zsh-hook zshaddhistory _t_rej";
    // run("echo shown"); run("echo secret1"); run("true"); send("echo s") → "hown"
}
#[test] fn alias_with_leading_space_is_not_suggested() {
    // rc_before "setopt histignorespace"; rc_after "alias sp=' echo hidden'";
    // run("sx=1"); run("sp"); run("true"); send("s") → "x=1" (were "sp" reported it would be "p")
}
#[test] fn repeated_hidden_line_is_not_suggested() {
    // rc_before "setopt histignorespace"; run("echo shown"); run(" echo secret"); run(" echo secret");
    // run("true"); send("echo s") → "hown"
}
#[test] fn reports_exit_status_and_start_cwd() {
    // recorder stub; run("mkdir sub; cd sub"); run("false"); run("true") →
    // wire.log has a line matching ^ran\x1f<HOME>/sub\x1f1\x1f[0-9]+\x1ffalse$
}
#[test] fn our_hook_never_rejects_a_line() { /* run("echo kept"); `fc -ln -1` inside the shell (dump to file) == "echo kept" */ }
#[test] fn history_file_appended_by_another_shell_is_seen() {
    // start; append "git commit -m wip\n" to histfile from the test; send("git c") → "ommit -m wip"
}
#[test] fn history_file_rewritten_without_a_line_drops_it() {
    // history ["rm -rf build", "ls"]; send("rm") → wait for " -rf build" (file loaded); "\x15" (⌃U);
    // rewrite the file to "ls\n" via temp + rename; send("rm"); then repeat "\x7f" + "m" every 100 ms
    // (each retype is a fresh request) until postdisplay == ""; must happen within 3 s
}
#[test] fn no_histfile_session_commands_still_suggested() { /* rc_after "unset HISTFILE"; run("echo hi"); run("true"); send("echo h") → "i" */ }
```

- [ ] **Step 2: Run** the adapter tests — Expected: new tests FAIL.
- [ ] **Step 3: Implement** the hooks with `add-zsh-hook zshaddhistory _fuxx_as_addhistory` and
  `add-zsh-hook preexec _fuxx_as_preexec` at load, and the check above in `_fuxx_as_precmd` (runs
  every prompt, before the first-prompt start, only while state is `on` — the first prompt has no
  command to report).
- [ ] **Step 4: Run** the adapter tests — Expected: PASS.
- [ ] **Step 5: Docs.** §5.8 zsh bullet: replace "The exact check is verified against zsh's docs in M0"
  with the check above in a few sentences (event-number rule, the filters, `HISTORY_IGNORE` checked
  by the adapter), keeping the user's decision (one prompt later) already written there. Progress log line.
- [ ] **Step 6: Commit:** `feat(zsh): report saved commands per §5.8`.

---

### Task 9: zsh 5.3.1 / 5.8.1 / 5.9 matrix (memo and exact-string fallback)

**Files:**
- Create: `scripts/build-zsh.sh`, `scripts/test-zsh-matrix.sh`
- Modify: `docs/design.md` §14, `cli/tests/zsh_adapter.rs` (only if a version-specific fix is needed)

**Interfaces:**
- Produces: `scripts/build-zsh.sh <version>` → `target/zsh/<version>/bin/zsh` (skips if present);
  `scripts/test-zsh-matrix.sh` → runs `cargo test -p fuxx-completes --test zsh_adapter` with
  `FUXX_TEST_ZSH` set to `target/zsh/5.3.1/bin/zsh`, `target/zsh/5.8.1/bin/zsh`, `/bin/zsh`, and
  exits non-zero if any run fails.

- [ ] **Step 1: Write `build-zsh.sh`.** Download `zsh-<ver>.tar.xz` from the zsh project's
  SourceForge files (`https://sourceforge.net/projects/zsh/files/zsh/<ver>/zsh-<ver>.tar.xz/download`;
  confirm the URL resolves first), print its SHA-256, unpack into `target/zsh/src/`, `./configure
  --prefix=$PWD/target/zsh/<ver> --disable-etcdir --disable-gdbm`, `make -j`, `make install.bin
  install.modules install.fns`. If 5.3.1 fails on current clang with implicit-declaration errors,
  add `CFLAGS="-Wno-error=implicit-function-declaration -Wno-error=implicit-int"`; record what was needed.
  (A maintainer tool; nothing the libraries ship.)
- [ ] **Step 2: Run** `scripts/build-zsh.sh 5.3.1 && scripts/build-zsh.sh 5.8.1 && target/zsh/5.8.1/bin/zsh -fc 'print $ZSH_VERSION'` — Expected: `5.8.1`.
- [ ] **Step 3: Run** `scripts/test-zsh-matrix.sh` — Expected: all adapter tests PASS on all three.
  `ghost_text_is_dimmed_and_tagged` and `foreign_region_highlight_entry_survives` are the memo/fallback
  proof. Any failure → superpowers:systematic-debugging, then fix the plugin, never weaken a test.
- [ ] **Step 4: Run** `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings` — Expected: PASS, clean.
- [ ] **Step 5: Docs.** §14: "Old zsh versions come from source builds (`scripts/build-zsh.sh`, cached
  in `target/zsh/`); the matrix runs with `scripts/test-zsh-matrix.sh` (AI's call, M0). bash and fish
  versions are picked in M2." Progress log line.
- [ ] **Step 6: Commit:** `test(zsh): matrix on zsh 5.3.1, 5.8.1 and 5.9`.

---

### Task 10: Manual check in a real terminal, and closing M0's docs

**Files:**
- Modify: `docs/roadmap.md`, `README.md`, `adapters/zsh/README.md`, `core/src/lib.rs`, `fuxx-autosuggestions/src/lib.rs` (doc comments: no longer "placeholder")

- [ ] **Step 1: Ask the user to check by hand** (the AI can't see a terminal; AGENTS.md). In a
  throwaway shell — `env -i HOME=$(mktemp -d) TERM=$TERM PATH=/usr/bin:/bin zsh -d -i`, then
  `HISTFILE=<a copy of a sample history>; FUXX_COMPLETES_BIN=<repo>/target/debug/fuxx-completes;
  source <repo>/adapters/zsh/fuxx-autosuggestions.plugin.zsh` — in Ghostty, iTerm and Terminal.app:
  grey text appears as you type; → / ⌃E / End accept; Esc f / ⌥→ take a word; ghost text never stays
  on screen after Enter; vi mode (`bindkey -v`) works; a narrow window and a multi-line command look
  right; nothing printed at startup; a command you run is suggested once you've run one more (one
  prompt later, the user's decision), and ` echo secret` with `setopt histignorespace` never is. Record what the user saw, per terminal, in the Progress log.
  powerlevel10k and fuxx itself are checked in M1/M3 (coexistence, plugin packaging).
- [ ] **Step 2: Run** `cargo test --workspace` and `scripts/test-zsh-matrix.sh` once more — Expected: PASS.
- [ ] **Step 3: Docs.** `docs/roadmap.md`: M0 → ✅ only after Step 1 is confirmed; "Current milestone:
  M1 (not started)"; a Progress log entry summarising M0 and the AI's calls. `README.md`: status line
  "pre-alpha: the zsh ghost text works from source (M0); nothing to install yet"; fix "How fuxx uses
  it" (it still says fuxx links the crates and has one switch per library — since 2026-11-13 fuxx
  installs this as a plugin with one On/Off switch, design §8); in the architecture sketch "proposal:
  line-delimited JSON" → "wire format 1", and "Rust crates (linked directly by fuxx)" changed to match
  the design §5 diagram. `adapters/zsh/README.md`: "Not built yet" → what M0 does and
  how to source it by hand.
- [ ] **Step 4: Commit:** `docs: M0 done`.

---

## Self-review (done 2026-11-13)

- **Coverage of §15 M0:** binary serves zsh history (Tasks 1–5); ghost text on 5.3+ with memo/fallback
  (6, 9); `forward-char`/`end-of-line` accept, `forward-word` word, vi widgets (7); End and ⌥→ only if
  unbound (7); reports per §5.8 (8); file re-read on change (4, 8); async (6); temp-HOME tests (all
  adapter tests, harness in 6). Not in M0, on purpose: context file, `doctor`, `forget`, coexistence,
  Warp, double-load guard, `FUXX_PLUGIN_DATA` (M1); `report`, bash, fish, host channel (M2);
  `fuxx-plugin.conf` and zips (M3); latency benchmark (M1).
- **Type consistency:** `History`, `HistoryFile::refresh(&mut History)`, `suggestion(&History, &str)`,
  `Session::handle/handle_error`, `serve(BufRead, Write)` and the `_fuxx_as_*` names are used the same
  way in every task.
- **Proportion:** bodies appear only for the FIFO open order (verified, not derivable) and the test
  assertions.

## For the user before execution (answered 2026-11-13)

- **Question:** a command another plugin's `zshaddhistory` hook rejects (status 1), or that an alias
  with a leading space hides, still sits in `$history` at `precmd`. Keep that as a documented gap, or
  report each command one prompt later? **The user chose (b): one prompt later** (Task 8, §5.8).
- **Design tension:** §5.6 says one plugin file per library (`fuxx-autosuggestions.plugin.zsh`), §8's
  manifest pointed at `shell/fuxx-completes.plugin.zsh`. **Settled by the user:** §8 now names
  `shell/fuxx-autosuggestions.plugin.zsh`.
- **Stale since 2026-11-13:** §5.5 "fuxx links the crates (§8)" (Task 1) and README "How fuxx uses
  it" (Task 10) are fixed by this plan's doc steps. The §5 diagram no longer says fuxx links the
  crates (fixed with the plan's commit, the user's word).
