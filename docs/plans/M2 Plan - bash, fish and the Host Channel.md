# M2 bash, fish and the Host Channel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Written 2026-11-22 from `docs/design.md` §15's M2 row (designed and approved 2026-11-09). Built in
the session (executing-plans), then one whole-branch review on the most capable model, one fix
pass and a test release for the user's check (the user's standing choice for milestones).
Decisions marked **AI's call** are open to change; each lands in `docs/design.md` in the task
named. **Four decisions are the user's** (below); the user approved all four AI's picks on
2026-11-23 ("go"), so the plan is built as written.

**Goal:** bash 4.0+ users get the best suggestion on an explicit key (bounded at 50 ms) with
history reported through `PROMPT_COMMAND`; the ble.sh path is decided; fish stays off and reports
its commands through `fuxx-completes report`; zsh and bash forward the buffer, cursor and facts to
a host terminal over OSC 7701 when `FUXX_COMPLETES_HOST=1`, and apply what the host accepts.

**Architecture:** No new ranking. The core learns bash's history file format
(`core/src/bash_history.rs`) and the host channel's encoding (`core/src/host.rs`, so a host such as
fuxx can link it). The binary gains `report` (one record, no history load) and `why --format`. Three
thin shell files: `adapters/bash/fuxx-autosuggestions.bash` (named pipes, explicit keys, sync
requests), `adapters/fish/fuxx-autosuggestions.fish` (reports only), and the zsh plugin's host
mode. Old and new shells are built into `target/` by scripts, as `scripts/build-zsh.sh` does.

**Tech Stack:** Rust 2021, std only (no new crate; base64 is hand-written); zsh 5.3+, bash 4.0+
(GNU readline `bind -x`), ble.sh 0.4 (devel), fish 3.0+.

**Spec:** `docs/design.md` §3, §5.5 (protocol, host channel), §5.6, §5.7, §5.8, §6 (bash, fish), §7,
§8, §9, §14, §16, §17 q2. Rules: `AGENTS.md`.

## What was checked, and where (2026-11-22)

| Fact | Source |
| --- | --- |
| `bind -x` sets `READLINE_LINE`/`READLINE_POINT` and reads them back; fractional `read -t` | bash `CHANGES`, bash-4.0-alpha items bb and y (Context7 `/websites/gnu_software_bash_manual_html_node` for the manual text) |
| `READLINE_POINT` counts **bytes** before bash 5.0, characters from 5.0 | bash `CHANGES`, bash-5.0-alpha item xxxx |
| `{var}>file` descriptor allocation exists only from bash 4.1 | bash `CHANGES`, bash-4.1-alpha item p |
| `PROMPT_COMMAND` may be an array from bash 5.1; `bind -x` per keymap from 5.1; `READLINE_MARK` from 5.1 | bash `CHANGES`, bash-5.1-alpha items ee, a, v |
| bash's history file: a first line `#<digit>` means timestamps; with timestamps, lines without a timestamp before them join the previous entry (multi-line); otherwise blank lines are skipped | bash `lib/readline/histfile.c` (`HIST_TIMESTAMP_START`, `history_multiline_entries`) |
| Latest bash 5.3; bash 4.0 tarball still on ftp.gnu.org | ftp.gnu.org/gnu/bash listing |
| ble.sh's auto-complete is **bash 4.0+ only** ("7.3 Auto completions (auto-complete) (Bash 4.0+)"); the manual documents no way to add a source | ble.sh wiki Manual §7 (Context7 `/akinomyoga/ble.sh`) |
| ble.sh's sources are the array `_ble_complete_auto_source=(history syntax)`, each `ble/complete/auto-complete/source:<name>`, which shows text with `ble/complete/auto-complete/enter h 0 <suggest> '' <cand>`; `ble/decode/has-input` tells pending input | ble.sh `lib/core-complete.sh` (master); atuin's `crates/atuin/src/shell/atuin.bash` uses exactly this, guarded by `_ble_version >= 400` |
| `ble-bind -x` gives `READLINE_LINE`/`READLINE_POINT` like `bind -x` | ble.sh wiki Manual §3.4.2 |
| ble.sh's newest release is `v0.4.0-devel3` (no stable 0.4); building from git needs gawk, which this Mac lacks; the release tarball needs no build | GitHub releases API; ble.sh README |
| `fish_postexec` gets the command line as `$argv[1]` and is not emitted for empty lines; `fish_private_mode`; `fish_history` names the session file `<name>_history`, empty = not saved; `fish_should_add_to_history` (4.0+) decides alone when defined, else a leading space hides a line; `disown` drops a job from fish's list | Context7 `/fish-shell/fish-shell` (language.rst, interactive.rst, cmds/history.rst, cmds/fish_should_add_to_history.rst, cmds/disown.rst) |
| fish releases: 3.0.0–3.0.2, latest 3.x 3.7.1, latest 4.x 4.9.3; fish 4 builds with Rust 1.85+ and CMake | GitHub releases API; fish README |
| This Mac: `/bin/bash` 3.2.57, no fish, no ble.sh, no gawk; cmake, rustc 1.97 and GNU make 3.81 present | local check |
| OSC 7701: a web search found no terminal or program using it | preliminary only; the real re-check is Task 6 Step 1 |

## Decisions only the user can make

| # | Question | Options and cost | **AI's pick** |
| --- | --- | --- | --- |
| U1 | The ble.sh path (§17 q2) | **B1** stay off when ble.sh is loaded (a detection line; ble.sh users keep its own history ghost text, without our context ranking). **B2** feed ble.sh's auto-complete through `_ble_complete_auto_source` (Task 4: about 150 lines of shell and tests; real as-you-type ghost text with our ranking in bash 4.0+; leans on an interface the ble.sh manual doesn't document, though atuin ships on it; needs AGENTS rule 6 to allow a second bounded wait: our source polls the core in 5 ms slices inside ble.sh's idle task and gives up the moment a key arrives) | **B2**: the only way bash ever gets ghost text, and §6 says to feed ble.sh if we can. If B1, Task 4 shrinks to the stay-off check inside Task 3 |
| U2 | Pruning fish's context file. `report` loads no history (§5.5), and a core only rewrites its own history's file (§5.7), so nothing ever deletes records of commands removed from fish's history | **F1** `report` keeps the 4 MiB cap (trims the oldest, down to 3 MiB) and §5.7 states the limit: a fish command deleted from history keeps its records (key, folder, status, time; no text) until the cap drops them. **F2** also add a fish history reader to the core and prune by history whenever `report` trims (about 150 lines and tests; still nothing below the cap) | **F1**: the records hold no text and can't be read back without the history; F2 adds a parser for a rare trim |
| U3 | The M1c list and the M1d fix in bash (design row: "insert the best suggestion") | **A1** the explicit key inserts the best suggestion; on an empty line after a failure the same key inserts the fix (free: the core already answers an empty `suggest` with it). **A2** also a key that prints the list of 5 below the line and picks with ⌥1–⌥5 (a modal loop and a new `sync` list request: the M7 menu's machinery, early) | **A1** |
| U4 | Test shells | **C1** Homebrew for the newest bash and fish (installs outside the repo). **C2** everything built or unpacked into `target/` like `scripts/build-zsh.sh`: bash 4.0 and 5.3 from GNU tarballs, fish 3.0.2 and 3.7.1 (CMake, C++) and 4.9.3 (cargo), ble.sh's release tarball; first build about 15–20 min. **C3** C2 without fish 3.0.2 and bash 4.0 (faster; the minimums untested) | **C2**, matrix: bash 3.2 (`/bin/bash`, stay-off tests only), 4.0, 5.3; ble.sh 0.4.0-devel3 on 5.3; fish 3.0.2, 3.7.1, 4.9.3. A version that won't build with the zsh script's kind of flag fixes is reported to the user, not patched |

### The user's decisions (2026-11-23)

The user's word: **"go"**, approving all four AI's picks.

| # | Decided | What it means for the build |
| --- | --- | --- |
| U1 | **B2**: feed ble.sh's own grey suggestions | Task 4 as written. AGENTS.md rule 6 now allows the second bounded wait: inside ble.sh's idle task our source checks for the core's answer in 5 ms slices, at most 50 ms, and gives up the moment a key arrives |
| U2 | **F1**: `report` keeps only the 4 MiB cap | design §5.7 states the limit: a fish command deleted from fish's history keeps its records (no text) until the cap drops them |
| U3 | **A1**: the key inserts the best suggestion | → and End at the end of the line insert it; on an empty line after a failure they insert the fix; no list in bash |
| U4 | **C2**: every test shell built into `target/` | `scripts/build-bash.sh`, `build-fish.sh`, `get-blesh.sh`; a version that won't build is reported to the user, not patched |

Not a decision here: fuxx's manifest has only a `zsh =` key (design §8), so fuxx keeps loading only
the zsh adapter; a `bash =` key, and fuxx drawing over the host channel, are fuxx's side and later.
`fuxx-plugin.conf` is unchanged in M2.

## Decisions this plan settles (AI's calls)

| Question | **AI's call** | Why | Lands in |
| --- | --- | --- | --- |
| bash file | `adapters/bash/fuxx-autosuggestions.bash`, sourced at the end of `~/.bashrc`; setup waits for the first prompt (§9) | One file per library (§5.6) | §5.6 |
| bash transport | Named pipes in a private `mkdir -m 700` temp folder, opened read-write first (as zsh); `exec {fd}<>` on 4.1+, on 4.0 the first free descriptor from 10 up (`[[ ! -e /dev/fd/N ]]`); unlinked at `ready` | §5.5 named this the likely choice; bash 4.0 has no `{var}` | §5.5 |
| Starting the core in bash | From `/`, `{ (exec "$bin" serve <req >resp 2>/dev/null) & } 2>/dev/null; disown $!` | No `[1] pid` or `Done` lines in an interactive bash | §5.5 |
| Writes never block bash | bash can't open a descriptor non-blocking: count bytes the core may not have read (as zsh below 5.9: an answer to a `suggest` means it read everything up to it) and refuse a write that could pass 6 KiB; any failed write stops the core (`write-failed`) | Hard rule 6 | §5.5 |
| The sync read | Before sending, drain stale lines (`read -t 0`); send `suggest … sync …`; read with what is left of 50 ms (bash 5.0+ `EPOCHREALTIME` deadline; 4.x one `read -t 0.05`); a wrong id or a timeout does nothing; EOF stops the core (`exited`) | §3's bound; no clock below 5.0 | §6 bash |
| Keys | → (`\e[C`, `\eOC`) and End (`\e[F`, `\eOF`, `\e[4~`) in `emacs` and `vi-insert`, each only while bound to its default (`forward-char`, `end-of-line`): at the end of the line they insert the suggestion (or the fix on an empty line); elsewhere → moves one character and End moves to the end, as before | → and End accept in zsh too; a user's own binding is kept (§9) | §6 bash |
| Cursor | Code points on the wire; bash < 5.0 converts `READLINE_POINT` bytes ↔ characters (`LC_ALL=C` lengths) | bash 5.0 changed the unit | §6 bash |
| `PROMPT_COMMAND` | Our function goes **first** (array element 0 on 5.1+ arrays, else prepended with `;`) and ends with `return $status`, so the later entries see the command's `$?` | Appended, it would read the previous entry's `$?` (design says "appended": corrected) | §5.8 |
| What a prompt reports | When `history 1` (with `HISTTIMEFORMAT` unset locally) shows a new number: `ran` (cwd saved at the previous prompt, status, time, text) and `prev␟text␟status`; no new number (empty Enter, a hidden line, `ignoredups`) sends nothing | §5.8; bash can't tell an empty Enter from a hidden line | §5.8 |
| Time in bash | `EPOCHSECONDS` (5.0+), `printf -v t '%(%s)T' -1` (4.2+), else one `date +%s` at setup plus `SECONDS` | No fork per prompt | §5.8 |
| `CDPATH` | Split on `:` into the `suggest` cdpath field; left off when `cd`/`pushd` is an alias or function, `shopt cdable_vars` is on, or a folder holds a control character | zsh's rule (§5.4) | §5.4 |
| bash guards | Interactive only; bash ≥ 4.0 (`BASH_VERSINFO`) else off `bash-too-old`; Warp off `conflict:warp`; ble.sh per U1; the double-load guard by the resolved `BASH_SOURCE` path (§8); a `fuxx-completes` function (doctor facts, `why --format bash --cwd --histfile`, the data folder) only when none exists | Same rules as zsh | §6, §7, §9 |
| bash history format | `HistoryFormat::Bash`; `#<digits>` first line = timestamped: timestamp lines dropped, lines between two timestamps join with `\n`; otherwise each non-blank line is an entry; CRLF trimmed | What bash itself does when it reads the file back | §5.1 |
| `why` for bash | `why --format zsh|bash` (default `zsh`) | `why` hard-codes zsh today | §7 |
| `report` | `fuxx-completes report --cwd DIR --status N [--time SECS] --histfile FILE -- COMMAND`; no `--time` = now; appends one record to `context/<own_file_id>` under the shared lock, retrying `try_lock` for at most 100 ms, else drops it; trims past 4 MiB to 3 MiB (U2 F1); never prints; exit 0 (2 on a usage error) | Short-lived, so no pending queue; fish needs no `date` fork | §5.5, §5.7 |
| fish file | `adapters/fish/fuxx-autosuggestions.fish` (for `conf.d/` or `source`); `fish_preexec` saves `$PWD`, `fish_postexec` reports with `$status` through `command $bin report … &>/dev/null &; disown $last_pid` | §6 fish; the cwd is where the command started | §6 fish |
| fish filters | No report when the line starts with a space (unless `fish_should_add_to_history` exists on fish 4.0+: then it alone decides), `fish_private_mode` is set, `fish_history` is set and empty, Warp; history path `${XDG_DATA_HOME:-$HOME/.local/share}/fish/<fish_history or fish>_history` (`default` = `fish`) | §5.8 | §5.8 |
| Host channel encoding | `ESC ] 7701 ; fuxx-completes ; 1 ; <message> ; <fields…> BEL`; numbers in plain digits, every other field base64 (RFC 4648, padded; empty stays empty); messages `hello` (shell, version, history format, history path, libraries), `buffer` (id, cursor, cwd, buffer), `clear`, `ran` (cwd, status, time, command), `prev` (command, status). `state` waits for M5, `listcfg` is not sent (the host has its own settings) | §5.5: "the same ones the adapter would send", plus `prev` since M1d | §5.5 |
| Accept payload | `ESC [ 7701 ~` + base64(id `US` buffer `US` cursor) + `BEL`; read in ≤ 50 ms; applied only when the id is the last forwarded `buffer`'s; anything else is read and dropped | §5.5 | §5.5 |
| Where host messages go | To the terminal (`$TTY` in zsh, `/dev/tty` in bash), never stdout. zsh writes only from ZLE: `hello`, `ran`, `prev` queue at `precmd` and go at `zle-line-init`; `buffer` from the wrapped widgets; `clear` at line-finish. bash writes from `PROMPT_COMMAND` (`hello` once, `ran`, `prev`, `clear`) and `buffer` at the explicit keys | Nothing at startup or in `precmd` (powerlevel10k's instant prompt) | §5.5, §6 |
| Host mode details | `FUXX_COMPLETES_HOST=1` and neither `TMUX` nor `STY` → host mode: no `serve`, nothing drawn; with `TMUX`/`STY` → normal mode; `ESC [ 7701 ~` bound only if free, in `emacs`/`viins` (zsh) and `emacs`/`vi-insert` (bash); base64 in pure shell, no fork; doctor fact `host=on|off|off:tmux|off:no-accept` | §5.5; one fork per keystroke would slow typing | §5.5, §7 |
| Codec home | `core/src/host.rs` (`HostMessage`, `encode_osc`, `decode_osc`, `scan`, `encode_accept`, `decode_accept`, `base64`), used by the fake host and any embedder | §5.1: protocol types live in the core | §5.5 |
| Test shells and harness | `scripts/build-bash.sh <ver>`, `scripts/build-fish.sh <ver>`, `scripts/get-blesh.sh <tag>` into `target/{bash,fish,blesh}/<ver>/` (sha256 printed, pinned where published); `scripts/test-bash-matrix.sh`, `scripts/test-fish-matrix.sh`; tests read `FUXX_TEST_BASH`, `FUXX_TEST_BLESH`, `FUXX_TEST_FISH`, defaulting to the newest built copy, and fail with the build command when none exists | §14; zsh's precedent | §14 |

## Global Constraints

- **bash:** explicit keys only; a `sync` request waits **at most 50 ms**, on timeout it does
  nothing and the late answer is dropped; keys that only type are never bound (design §3).
- Core answers stay under **10 ms at p95** on the 100,000-line benchmark, bash history included.
- **Versions:** zsh 5.3+, plain-readline bash 4.0+, fish 3.0+ (`fish_should_add_to_history` from
  4.0). Below a minimum: off, silent, `doctor` says why.
- **Nothing printed** at shell startup or at a prompt, in any shell, ever (no job lines either).
- Buffers over **1000 characters**, or holding `0x1F`/`0x1E`, are never sent (core or host).
- Host channel only with `FUXX_COMPLETES_HOST=1` and no `TMUX`/`STY`; without it no `ESC ] 7701`
  ever reaches the terminal.
- **No new dependency.** Release binary ≤ **786,432 bytes** (`cli/tests/size.rs`).
- History files read only; the context file holds no command text; tests use a temp HOME,
  `XDG_*`, `INPUTRC` and generated history, never the user's.
- Per-task gate: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --check`, plus the matrices of the shells the task touched
  (`scripts/test-zsh-matrix.sh`, `scripts/test-bash-matrix.sh`, `scripts/test-fish-matrix.sh`).
  Docs (design, roadmap Progress log, adapter README) with every task. Branch `m2-bash-fish-host`;
  no merge, push, tag or release without the user's word.

## Review Focus

1. **Multibyte lines in bash 4.x**, where `READLINE_POINT` is bytes: accepting after `é` or an
   emoji must land at the end with the right cursor, and the wire's cursor is code points. Test
   `multibyte_buffer_accepts_cleanly` in Task 3.
2. **A user's `PROMPT_COMMAND`**: an array (5.1+), a string reading `$?`, bash-preexec: they still
   see the command's `$?`, and we report the right status. Test `prompt_command_keeps_the_status`
   in Task 3.
3. **Hostile or broken host traffic**: a buffer with `;`, `BEL`-like bytes, quotes, emoji; an accept
   with a stale id, bad base64 or no `BEL` within 50 ms: nothing applied, no stray characters typed.
   Tests `hostile_text_round_trips` (Task 6, core) and `stale_or_broken_accept_does_nothing`
   (Task 6, zsh and bash).
4. **Job noise and waiting**: bash's `[1] 1234`/`Done`, fish's "Job … has ended", a fish prompt
   waiting on a slow `report`. Tests `prints_nothing_at_startup_or_prompt` (Task 3) and
   `reports_never_hold_the_prompt` (Task 5).
5. **Staying off silently**: bash 3.2, Warp, ble.sh per U1, fish private mode, a user's own →
   binding, host mode inside tmux. Tests `stays_off_silently` (Task 3), `filters_and_off_cases`
   (Task 5), `tmux_falls_back_to_normal_mode` (Task 6).

---

## File map

| File | Change |
| --- | --- |
| `scripts/build-bash.sh`, `build-fish.sh`, `get-blesh.sh`, `test-bash-matrix.sh`, `test-fish-matrix.sh` (new) | test shells in `target/` |
| `cli/tests/common/{mod.rs → pty.rs, zsh.rs}`, `bash.rs`, `fish.rs` (new) | one pty core, three harnesses |
| `core/src/bash_history.rs` (new), `history.rs`, `history_file.rs` | bash format |
| `core/src/host.rs` (new), `lib.rs` | host channel codec |
| `core/src/context/store.rs` | `trim_to_cap` |
| `cli/src/main.rs`, `report.rs` (new), `doctor.rs`, `session.rs` | `report`, `why --format`, bash/fish facts, `host` fact |
| `adapters/bash/fuxx-autosuggestions.bash` (new) | the bash adapter (and ble.sh source, U1) |
| `adapters/fish/fuxx-autosuggestions.fish` (new) | the fish adapter |
| `adapters/zsh/fuxx-autosuggestions.plugin.zsh` | host mode |
| `cli/tests/{bash_adapter,fish_adapter}.rs` (new), `zsh_adapter.rs`, `cli.rs`, `latency.rs` | tests |
| `docs/design.md`, `docs/roadmap.md`, `README.md`, `adapters/*/README.md` | per task |

---

### Task 1: Test shells and the pty harness

**Interfaces — Produces:** `common::pty::Pty` (`spawn(cmd: Command, size) -> Pty`, `send(&str)`,
`wait_screen(&str) -> bool`, `run(line, prompt)`, `screen_now() -> String`, `home() -> &Path`),
with the clean environment (`env_clear`, temp `HOME`, `XDG_*`, `PATH=/usr/bin:/bin`, `TERM`,
`LANG`); `ZshHarness` unchanged for its callers; `BashHarness::start(BashOpts { history, rc_before,
rc_after, core_script, core_bin, env: Vec<(&str, String)>, blesh: bool, size })` running
`$FUXX_TEST_BASH --noprofile --rcfile $HOME/.bashrc -i` with `INPUTRC=$HOME/.inputrc` (empty),
`HISTFILE=$HOME/.bash_history`, `PS1='READY> '` and a `bind -x '"\e[9999~": …'` dump of
`READLINE_LINE`/`READLINE_POINT`/`_FUXX_AS_STATE`/`_FUXX_AS_WHY` (`state() -> BashState`);
`FishHarness::start(FishOpts { history, config_before, core_script, env, size })` with
`XDG_CONFIG_HOME/fish/config.fish` and a prompt function printing `READY> `;
`common::bash_path()`, `fish_path()`, `blesh_path()`.

- [x] **Step 1:** write `scripts/build-bash.sh` (bash 4.0 and 5.3 from `ftp.gnu.org/gnu/bash`, configure
  `--prefix=target/bash/<ver> --without-bash-malloc`, clang flags as in `build-zsh.sh`; "built" =
  the binary runs `bind -x` and `read -t 0.01`), `scripts/build-fish.sh` (3.0.2 and 3.7.1 with CMake
  into `target/fish/<ver>`; 4.9.3 with `cargo build --release` and `CARGO_TARGET_DIR` under
  `target/fish/build`), `scripts/get-blesh.sh` (`ble-0.4.0-devel3.tar.xz` into `target/blesh/`);
  run them. A version that won't build: stop and tell the user (U4).
- [x] **Step 2: Failing tests** `cli/tests/bash_adapter.rs::harness_runs_bash_in_a_temp_home` (the prompt
  shows; `echo $HOME` prints the temp folder; `state()` returns buffer `echo x` after typing it) and
  `cli/tests/fish_adapter.rs::harness_runs_fish_in_a_temp_home` (same, `echo $HOME`).
- [x] **Step 3:** split `common/mod.rs` into `pty.rs` + `zsh.rs`, add `bash.rs`, `fish.rs`; the two
  matrix scripts. Run: both tests pass; `scripts/test-zsh-matrix.sh` still passes (no zsh test changed).
- [x] **Step 4:** gate. **Docs:** design §14 (the versions and how they're obtained), roadmap.
  Commit `test: bash and fish in target/, one pty harness for three shells`.

### Task 2: The core and binary for bash

**Interfaces — Produces:** `HistoryFormat::Bash` (`"bash"` parses); `pub struct
bash_history::BashHistoryParser` with `new()`, `feed(&mut self, bytes: &[u8]) -> Vec<String>`
(complete entries; holds a partial line and an open multi-line entry), `finish(self) -> Vec<String>`;
`HistoryFile` dispatching on the format; `Session` serving `hello … bash …`; `why --format
zsh|bash`; doctor facts keyed by `shell=` (zsh, bash, fish).

- [x] **Step 1: Failing tests** (`bash_history.rs`): `plain_lines_are_entries` (`"ls\n\ngit status\n"`
  → `["ls", "git status"]`); `timestamps_are_dropped` (`"#1700000000\nls\n#1700000001\ngit st\n"` →
  `["ls", "git st"]`); `multiline_entries_join_between_timestamps` (`"#1\nfor i in 1 2\ndo echo $i\ndone\n#2\nls\n"`
  → `["for i in 1 2\ndo echo $i\ndone", "ls"]`); `hash_without_digit_is_a_command` (`"#foo\n"` →
  `["#foo"]`); `chunks_resume_mid_entry` (the multi-line input fed in 3-byte chunks gives the same);
  `crlf_is_trimmed`. (`history.rs`): change `"bash".parse::<HistoryFormat>().is_err()` to `==
  Ok(HistoryFormat::Bash)`. (`session.rs`): `bash_hello_serves_a_bash_history` (timestamped file with
  `git status`; `suggest` `git s` → `tatus`). (`cli.rs`): `why_reads_a_bash_history` (`why --format bash
  --histfile f -- git status` prints `in your history`). (`doctor.rs`): `report_with_bash_facts`
  (`shell        bash 5.3.0(1)-release`; the no-facts hint names zsh and bash).
- [x] **Step 2:** run (fail); implement; run (pass).
- [x] **Step 3:** `latency.rs` gains `sync_requests_on_a_bash_history` (100,000 timestamped entries,
  2,000 `sync` requests through `serve_with`: p95 < 10 ms, max < 50 ms). Gate. **Docs:** design §5.1
  (bash format as built), §7 (`why --format`), roadmap. Commit `feat(core): read bash's history file`.

### Task 3: The bash adapter (plain readline)

**Interfaces — Consumes:** Task 2's `hello bash`. **Produces:** `adapters/bash/fuxx-autosuggestions.bash`;
globals `_FUXX_AS_STATE` (`''`/`on`/`off`), `_FUXX_AS_WHY` (`no-binary`, `start-failed`, `exited`,
`protocol`, `write-failed`, `bash-too-old`, `conflict:warp`, `conflict:ble.sh`), `_FUXX_AS_FILE`,
`_FUXX_AS_KEYS` (assoc, as zsh); functions `_fuxx_as_send`, `_fuxx_as_sync_suggest` (sets `REPLY` to
the text after the buffer, or returns 1), `_fuxx_as_key_right`, `_fuxx_as_key_end`,
`_fuxx_as_prompt` (the `PROMPT_COMMAND` entry), `fuxx-completes`.

- [x] **Step 1: Failing tests** (`bash_adapter.rs`, bash 4.0 and 5.3): `right_arrow_at_the_end_inserts`
  (history `git status`; `git s` →: buffer `git status`, point 10); `right_arrow_mid_line_moves`
  (`git s` ←← →: buffer `git s`, point 4); `end_inserts_at_the_end`; `multibyte_buffer_accepts_cleanly`
  (Review Focus 1: history `echo héllo wörld`; `echo hé` → → `echo héllo wörld`, point 16; with the
  recorder stub the `suggest` line's cursor field is `7`); `slow_core_does_nothing_within_50ms` (stub
  answers `ready`, then answers each `suggest` after 1 s: → leaves `git s` and the dump comes back
  within 200 ms; the next → after the late answer arrived gets its own answer, not the stale one);
  `reports_follow_history` (recorder: `ls`, `false`, ` hidden` with `HISTCONTROL=ignorespace`, `pwd`
  with `HISTIGNORE=pwd` → `ran␟<home>␟0␟…␟ls`, `ran␟<home>␟1␟…␟false`, `prev␟false␟1`, nothing for the
  other two); `cwd_is_where_the_command_started` (`cd sub` → `ran␟<home>␟…`);
  `prompt_command_keeps_the_status` (Review Focus 2: a user entry writing `$?` to a file, as a string
  and, on 5.1+, as an array; after `false` it wrote `1` and our `ran` says `1`);
  `fix_on_the_empty_line` (`ls`, then `lss` (127): → on the empty line → buffer `ls`, nothing ran);
  `cdpath_is_forwarded` (`CDPATH=/a:/b` → last field `/a␞/b`; `cd(){ builtin cd "$@"; }` → no field);
  `stays_off_silently` (Review Focus 5: `/bin/bash` 3.2 → `off`/`bash-too-old`, no core started;
  `TERM_PROGRAM=WarpTerminal` → `conflict:warp`; `bind '"\e[C": backward-char'` in the rc → → still
  moves back and `_FUXX_AS_KEYS` says `kept`); `prints_nothing_at_startup_or_prompt` (Review Focus 4:
  the screen up to the first prompt and after 5 commands holds no `[1]`, `Done` or other text than
  the prompts and command output; same after the core is killed); `core_death_goes_quiet` (kill the
  core: → does nothing, `doctor` says `exited`); `a_core_that_stops_reading_never_blocks` (stub reads
  `hello` only; 300 commands run, each prompt returns, state ends `off`/`write-failed`);
  `second_copy_stays_off`; `doctor_and_why_through_the_function` (`shell        bash 5.3…`; `why`
  passes `--format bash --cwd <home> --histfile <home>/.bash_history`).
- [x] **Step 2:** run (fail); implement per the AI's calls above; run (pass on 4.0, 5.3; the
  3.2 cases on `/bin/bash`).
- [x] **Step 3:** gate + `scripts/test-bash-matrix.sh`. **Docs:** design §5.5 (bash transport),
  §5.8 (bash reporting, `PROMPT_COMMAND` first), §6 bash (keys, cursor, limits: no ghost text,
  `ignoredups` repeats aren't counted, an rc that overwrites `PROMPT_COMMAND` after us turns
  reporting off), §7, §9; `adapters/bash/README.md` and `README.md` (install, keys, limits);
  roadmap. Commit `feat(bash): the bash adapter: explicit keys, sync requests, history reports`.

### Task 4: The ble.sh path (U1 = B2; with B1 this is one stay-off test in Task 3)

**Interfaces — Consumes:** `_fuxx_as_send`, the transport (Task 3). **Produces:**
`ble/complete/auto-complete/source:fuxx-autosuggestions` (registered with
`ble/util/import/eval-after-load core-complete 'ble/array#unshift _ble_complete_auto_source
fuxx-autosuggestions'`), only when `BLE_VERSION` is set, `_ble_version >= 400`, bash ≥ 4.0, and
`_ble_complete_auto_source` holds no `atuin-history` (then off, `conflict:atuin`); the plain-readline
keys are not bound under ble.sh.

- [x] **Step 1: Failing tests** (bash 5.3 + `FUXX_TEST_BLESH`): `blesh_shows_our_ranking` (history
  `git stash` then `git status`, context record: `git stash` succeeded in the cwd → typing `git st`
  then End (ble.sh's `auto_complete/@end insert`) gives `git stash`; ble.sh alone would give
  `git status`); `blesh_gives_way_to_typing` (stub answers after 1 s: `abc` typed at once is in the
  buffer within 100 ms; no ghost text); `blesh_beside_atuin_stays_off`; `blesh_reports_commands`
  (`ran` recorded under ble.sh); `blesh_on_bash_3_2_stays_off`.
- [x] **Step 2:** run (fail); implement: the source returns 1 unless the cursor is at the end; sends
  `suggest … sync …`; polls the reply in `read -t 0.005` slices up to 50 ms, returning 148 at once when
  `ble/decode/has-input` is true; shows the answer with `ble/complete/auto-complete/enter h 0 <rest> ''
  <whole>`. Run (pass).
- [x] **Step 3:** gate + bash matrix. **Docs:** design §3 and §6 bash (ble.sh: bash 4.0+ only, the
  internal interface, the poll), §9 row, §17 q2 answered, AGENTS.md rule 6 (the ble.sh poll, with the
  user's approval of U1); roadmap. Commit `feat(bash): feed ble.sh's auto-complete`.

### Task 5: fish and `fuxx-completes report`

**Interfaces — Produces:** `report` (see the AI's calls); `pub fn store::trim_to_cap(root: &Path, id:
&str) -> io::Result<bool>` (exclusive `try_lock`; past 4 MiB keeps the newest down to 3 MiB);
`adapters/fish/fuxx-autosuggestions.fish` with `_fuxx_as_preexec`, `_fuxx_as_postexec` and a
`fuxx-completes` function (doctor facts: `shell=fish`, `version`, `state=off`,
`why=fish-has-its-own`, `reporting=on|off:<why>`).

- [x] **Step 1: Failing tests** (`cli.rs`): `report_appends_one_record` (temp data; `report --cwd /x
  --status 1 --time 5 --histfile /h -- 'make test'` → the file `context/<own_file_id(secret, /h)>`
  holds one record with status 1, time 5, folder `/x`; no context file contains `make test`);
  `report_without_time_uses_now`; `report_is_silent_without_data` (no `HOME`: exit 0, no output);
  `report_usage_errors_exit_2`; `report_trims_past_the_cap` (own file prefilled past 4 MiB → ≤ 3 MiB,
  the new record present). (`fish_adapter.rs`, each fish, the binary replaced by a recorder script
  that appends its arguments to `wire.log`): `commands_are_reported` (`ls`, `false` → `report --cwd
  <home> --status 1 --histfile <home>/.local/share/fish/fish_history -- false`);
  `cwd_is_where_it_started` (`cd sub`); `filters_and_off_cases` (Review Focus 5: ` ls`, `set -g
  fish_private_mode 1`, `set -g fish_history ''`, `TERM_PROGRAM=WarpTerminal` → nothing; on 4.0+ a
  `fish_should_add_to_history` returning 1 → nothing, returning 0 → ` ls` is reported);
  `reports_never_hold_the_prompt` (Review Focus 4: recorder sleeps 1 s; 5 commands each return to the
  prompt within 300 ms; the screen never shows `has ended` or a job line); `never_starts_serve`
  (no `serve` in `wire.log`); `doctor_through_the_function` (`shell        fish 3.7.1`, `state
  off (fish has its own autosuggestions)`, `reporting    on`).
- [x] **Step 2:** run (fail); implement; run (pass on 3.0.2, 3.7.1, 4.9.3).
- [x] **Step 3:** gate + `scripts/test-fish-matrix.sh`. **Docs:** design §5.5 (`report` as built), §5.7
  (`report` and the cap; U2's stated limit), §5.8 and §6 fish (as built), §7 (usage line); 
  `adapters/fish/README.md`, `README.md`; roadmap. Commit `feat(fish): report commands through fuxx-completes report`.

### Task 6: The host channel (zsh and bash)

**Interfaces — Consumes:** Tasks 3 and 5's adapters. **Produces** (`core/src/host.rs`): `pub enum
HostMessage { Hello { shell, version, history_format, history_path: Option<PathBuf>, libraries:
Vec<String> }, Buffer { id: u64, cursor: usize, cwd: PathBuf, buffer: String }, Clear, Ran { cwd,
status: i32, time: i64, command }, Prev { command: Option<String>, status: Option<i32> } }`; `pub fn
encode_osc(&HostMessage) -> Vec<u8>`; `pub fn decode_osc(body: &[u8]) -> Option<HostMessage>` (body =
between `ESC ]` and `BEL`); `pub fn scan(stream: &[u8]) -> Vec<HostMessage>`; `pub fn
encode_accept(id: u64, buffer: &str, cursor: usize) -> Vec<u8>`; `pub fn decode_accept(payload: &[u8])
-> Option<(u64, String, usize)>`; `pub fn base64_encode(&[u8]) -> String`, `base64_decode(&str) ->
Option<Vec<u8>>`. Shell: `_fuxx_as_host_send <message> <fields…>`, the accept widget
`fuxx-as-host-accept` (zsh) and `_fuxx_as_host_accept` (bash).

- [x] **Step 1: The OSC number re-check.** Look for 7701 in xterm's ctlseqs, iTerm2, WezTerm, kitty,
  foot, Ghostty, VS Code (633), Windows Terminal, hterm, Konsole, rxvt-unicode, and tmux's passthrough
  rules; record the date, sources and result in design §5.5. A clash: stop and ask the user.
- [x] **Step 2: Failing tests** (`host.rs`): `base64_matches_rfc4648` (`""`, `"f"`→`Zg==`, `"fo"`→`Zm8=`,
  `"foo"`→`Zm9v`, `"foobar"`→`Zm9vYmFy`; bad input → `None`); `osc_round_trips_every_message`;
  `hostile_text_round_trips` (Review Focus 3: buffer `echo "a;b" '\a' é 🙂` and a cwd with `;` →
  the encoded body holds only `[A-Za-z0-9+/=;-]` and digits, and decodes equal);
  `other_namespaces_and_versions_are_ignored` (`7701;other;1;…`, `7701;fuxx-completes;2;…` → `None`);
  `scan_finds_messages_among_text` (other OSCs, plain text, a `ESC \` terminator, a cut-off
  sequence); `accept_round_trips`.
- [x] **Step 3: Failing tests** (pty; zsh on the zsh matrix, bash 4.0 and 5.3): `host_mode_forwards_facts_and_buffer`
  (`FUXX_COMPLETES_HOST=1`; type `git s` → `scan` gives `Hello { shell: "zsh" | "bash", … }`, and in zsh
  `Buffer { cursor: 5, buffer: "git s", cwd: home, .. }` (bash: after →); Enter → `Clear`; at the next
  line `Ran` and `Prev`); `host_mode_starts_no_core_and_draws_nothing` (recorder never sees `hello`;
  zsh `POSTDISPLAY` empty); `accept_from_host_sets_the_line` (the test writes `encode_accept(last
  Buffer id, "git status", 10)` → buffer `git status`, cursor 10; this is the read-path check §5.5
  asks for: if a shell version can't get the bytes after the CSI, its accept is off,
  `host=off:no-accept`, and the user is told); `stale_or_broken_accept_does_nothing` (Review Focus 3:
  an old id, bad base64, no `BEL` → buffer unchanged after 100 ms and no stray characters on screen);
  `no_sequence_without_the_variable` (unset → the screen never holds `\x1b]7701`);
  `tmux_falls_back_to_normal_mode` (Review Focus 5: `TMUX=/tmp/x` → no OSC, zsh ghost text works);
  `nothing_before_the_first_prompt` (no OSC before `READY> `).
- [x] **Step 4:** run (fail); implement; run (pass); zsh and bash matrices.
- [x] **Step 5:** gate. **Docs:** design §5.5 (encoding, accept read path as verified per version, the
  re-check), §6 zsh/bash host mode, §7 (`host` fact); README (host mode for terminal authors);
  roadmap. Commit `feat: the host channel over OSC 7701 (zsh, bash)`.

### Task 7: Measure, review, fix, test release

- [x] `cargo test --release -p fuxx-completes --test latency -- --ignored --nocapture` and `--test size
  -- --ignored --nocapture`; numbers into design §3. Over 786,432 bytes: stop and ask the user.
- [x] All three matrices; design §15 row and status line; roadmap row "built and reviewed, waiting
  for the user's check".
- [x] One whole-branch review (superpowers:requesting-code-review, most capable model, this Review
  Focus); one fix pass, each fix test-first; gate and matrices again.
- [ ] Test release v0.0.13 when the user says so (tagged on this branch, as v0.0.9–v0.0.11; v0.0.12 went to a chore on `main` on 2026-11-23).

## Self-review (2026-11-22)

- Spec coverage: §15 M2 row: bash 4.0+ keys, `sync` ≤ 50 ms, `PROMPT_COMMAND` (T3); ble.sh decided
  (U1, T4); fish off and `report` (T5); host channel forwarding, accept key, number re-check, fake
  host tests (T6). §5.1 bash format (T2); §5.7 for `report` (T5, U2); §7 doctor/why (T2, T3, T5, T6);
  §9 Warp, ble.sh, double load (T3, T5); §14 matrix and host tests (T1, T6); §17 q2 (T4).
- Types: `HistoryFormat::Bash` (T2) used by T3's `hello`; `_fuxx_as_send` (T3) used by T4 and T6;
  `HostMessage`/`scan`/`encode_accept` (T6) used by its pty tests; `trim_to_cap` (T5) only by `report`.
- Not in M2 (design): the `state` message (M5), menus (M6, M7), fuxx drawing (later), Linux.
