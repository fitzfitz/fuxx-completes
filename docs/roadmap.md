# Roadmap

The milestones from [`design.md`](design.md) §15. One at a time, in order. Status: ⬜ not started ·
🟨 in progress · ✅ done.

Building starts after fuxx's v0.5.0 release.

**Current milestone: M1b** (plan written 2026-11-22 and built at once, the user's "just go ahead with your best recommendation pick").

| | Milestone | Status |
| --- | --- | --- |
| M0 | Core + history + zsh adapter | ✅ done 2026-11-21 (the user's check: "close m0, all good") |
| M1 | Autosuggestions: context file and coexistence (now with frequency ranking, no boost for `cd`/`pushd`/`open`) | ✅ done 2026-11-22 ([plan](plans/M1%20Plan%20-%20Context%20File%20and%20Coexistence.md); the user's check on v0.0.7) |
| M1b | Project knowledge (added 2026-11-21; [design](superpowers/specs/2026-11-21-project-knowledge-design.md)) | 🟨 being built ([plan](plans/M1b%20Plan%20-%20Project%20Knowledge.md)) |
| M2 | bash and fish adapters, and the host channel (autosuggestions) | ⬜ |
| M3 | Release fuxx-autosuggestions | ⬜ |
| M4 | Spec format + Fig import | ⬜ |
| M5 | Line parsing, generators and shell state | ⬜ |
| M6 | zsh menu | ⬜ |
| M7 | bash and fish menus | ⬜ |
| M8 | Release fuxx-autocomplete | ⬜ |

What "done" means for each is in `design.md` §15. **fuxx installs fuxx-completes as a fuxx plugin**
(since 2026-11-13, `design.md` §8): M1 adds `FUXX_PLUGIN_DATA` and the double-load guard, M3 adds
the `fuxx-plugin.conf` and the plugin zips. fuxx's side (its plugin system, M11y–M11z) is tracked in
fuxx's own roadmap.

## Progress log

Newest first. Add an entry whenever a task is ready.

- **2026-11-22** — **M1b whole-branch review and its fix pass.** One fresh reviewer (Opus): ready
  with fixes, no Critical. Fixed, each with a test seen failing first: (1) inside a pnpm, yarn or
  bun workspace package the npm commands were offered (`npm install` damages a pnpm workspace):
  a data field `beats_below` lets the manager found up the walk take over; (2) a detection cut
  short by its 50 ms budget was cached as complete: `Detection::complete`, retried up to 3
  times; (3) a stuck detection switched project help off for the session: given up after 2 s,
  and files are opened non-blocking and checked on the open file; regraded from Minor: (4)
  `db:migrate:` in a Taskfile became `task db` (a wrong command); (5) duplicate removal was
  quadratic and names unbounded (~1 s detections): a set, and at most 500 names per file; (6) the
  "cold" benchmark was warm: it now loads history elsewhere. zsh matrix 127 × 3 passed before the
  fixes; the workspace passes after; release: cold 13 µs, warm p95 0.26 ms; binary 602,496 bytes
  (+97 KB). Deferred (minor): a file that failed to read, or a path checked with a `/`
  (`bin/rails`), isn't stamped, so adding one may go unnoticed until its folder changes; a test
  pinning neutral-only commands here above other history when there is no project; M1's slow
  concurrency stress test.

- **2026-11-22** — **M1b Task 5: the three-step order, wired in.** `History::ranked_here` (this
  folder's history: records here, not failed-only), `suggestion()` takes the project candidates
  and walks this folder's history, then project commands, then the rest as **one** sequence (a
  ruling: three separate walks broke M1's held-`cd` rule, caught by M1's own test), and `Session`
  owns a `ProjectCache`. `cli/tests/latency.rs` adds a cold/warm project benchmark;
  `cli/tests/size.rs` keeps the binary under 656,400 bytes. `design.md` §3, §5.4. Tests first, seen
  failing (the new signature): this folder's history beats project commands, project commands beat
  history from elsewhere, other history still shows without a project match, failed-only here
  doesn't hide project commands, ⌃C'd here counts, an exact project command is skipped, no secret
  means projects before other history, and a cold folder fills in on a later request. Release:
  cold 13 µs, warm p95 0.29 ms; binary 585,984 bytes (+83 KB).

- **2026-11-22** — **M1b Task 4: the project cache.** `core/src/projects/cache.rs`:
  `ProjectCache::candidates` serves a fresh entry (a few `stat`s), starts a detection on one
  worker thread for a missing or stale one (50 ms budget) and serves the stale entry meanwhile; 64
  folders, least recently used dropped. `design.md` §3. Tests first, seen failing (missing
  items): cold then filled, fresh not re-detected, a changed script and a deleted Makefile picked
  up, stale served while re-detecting, the 64-folder limit, one detection in flight, 1,000 warm
  calls under 50 ms.

- **2026-11-22** — **M1b Task 3: detection.** `core/src/projects/detect.rs`: the walk-up (8
  folders, `$HOME` and `/` never scanned, stop after `.git`), matching, `beats` per folder,
  candidates nearest first, stamps, the deadline. Fixtures for all 39 ecosystems
  (`core/tests/fixtures/projects/`, copied to a temp folder with a `.git` marker because git can't
  track a file named `.git`). `design.md` §5.4. Tests first, seen failing (missing items): cargo,
  pnpm beats npm, monorepo order, the walk's stops, 8 levels, a past deadline, an unreadable parent,
  hostile script names and a FIFO named `Makefile` (under 200 ms), stamps; and every ecosystem
  detected from its fixture with its first command and every template applied to `fx`.

- **2026-11-22** — **M1b Task 2: the six readers.** `core/src/projects/readers.rs`: `safe_name`,
  `read_small` (regular files ≤ 256 KB only), `json_keys` (a small JSON/JSONC parser), `toml_keys`,
  `make_targets`, `just_recipes`, `yaml_keys`, `file_names`. `design.md` §5.4. Tests first, seen
  failing (missing items): name rules, scripts in order, nested/JSONC, array names, unsafe names
  dropped, TOML tables and `[[bin]]`, Makefile and justfile rules, YAML blocks (flow style gives
  none), file names, and a big file, a folder and a FIFO skipped (the FIFO in under 100 ms).

- **2026-11-22** — **M1b Task 1: mappings as data.** `projects/*.toml` (39 project types in 8
  files, with a README) and `core/build.rs` (build-only `toml` 1.2 and `serde`) compile them into
  `core/src/projects` static tables; bad data fails the build (checked by hand with an unknown
  reader). `design.md` §5.4. Tests first, seen failing (no table): every planned ecosystem, unique
  ids and known `beats`, placeholders in templates, the pnpm entry, xcode/flutter/android shapes.

- **2026-11-22** — **M1b planned and started.** Plan: [`plans/M1b Plan - Project Knowledge.md`](plans/M1b%20Plan%20-%20Project%20Knowledge.md)
  (6 tasks, 39 ecosystems). The user: "just go ahead with your best recommendation pick", so it is
  built at once in the session, with one whole-branch review at the end.

- **2026-11-22** — **M1 done.** The user installed v0.0.7 in fuxx and checked it ("its good");
  `fuxx-completes doctor` there showed: on, fuxx's plugin copy, every free key bound, data under
  fuxx's plugin-data folder (`FUXX_PLUGIN_DATA`), one context file of 0.5 KB, the secret present.
  M1 is ✅ and merged to `main`. Next: the M1b plan, from its spec.

- **2026-11-21** — **v0.0.7 test release (the AI's pick on the user's "go with your picks"),
  before the M1 check.** v0.0.6 plus the ⌃C decision, so the user checks the final behaviour
  once. Same packaging and branch as v0.0.6; `main` untouched.

- **2026-11-21** — **⌃C is neutral (the user's decision: "neutral").** The review's open question:
  a command stopped with ⌃C (`npm run dev`, `tail -f`) counted as failed and ranked last. Now exit
  status 130 (⌃C) and 143 (SIGTERM) count as neither a success nor a failure; other non-zero
  statuses (129, 137, ...) still count as failures. `design.md` §4, §5.4; README. Tests first,
  seen failing: the tally ignores 130 and 143 but counts 129, and a dev server stopped with ⌃C
  ranks like a command with no records, not last. Not in v0.0.6; released in v0.0.7.

- **2026-11-21** — **v0.0.6 test release (the user's call: "publish it"), before the M1 check.**
  Like v0.0.1–v0.0.5: `fuxx-plugin-macos-arm64.zip` (Apple Silicon, zsh), installed through
  fuxx's Settings → Plugins. Tagged on the M1 branch; `main` stays at the last checked state
  until the user's check. Release size settings added (the user's "size and speed first"; same
  behaviour): `strip`, `lto`, one codegen unit, no `panic = "abort"` (a panicking worker must not
  end the core). The binary went from 763 KB (M1 without them) to 491 KB, below v0.0.5's 513 KB;
  benchmark p95 34 µs. M1b's 150 KB limit is measured from this binary.

- **2026-11-21** — **M1 whole-branch review and its fix pass.** One fresh reviewer (Opus) on the
  whole branch: ready with fixes; no Critical. Fixed, each with a test seen failing first:
  (1) a `forget` missed when the new key reuses the old inode (ext4): the key's bytes are now
  compared; (2) the `SHARE_HISTORY` retire rule let old entries from other shells absorb this
  shell's report, so the command just run no longer ranked first with zsh's defaults: only
  entries read since the `ran` before the previous one count; (3) `doctor`'s `shglob` warning
  could never fire (read under `emulate`): the function now reads the user's option first;
  (4, regraded from Minor) a missing or unreadable history file let compaction delete every
  record: only a complete read counts as loaded; (5, regraded from Minor) `forget` now removes
  the key before the folder. `Cargo.lock` committed with `blake3`. Taken to the user: whether a
  command stopped with ⌃C (status 130/143) should count as failed. Found on the way, not ours:
  in the pty harness, with `setopt shglob` a typed line doesn't run even with the plugin off, so
  the `shglob` test runs zsh as a process. Deferred (minor): a rename racing a growth read can double-count
  a file until the next replacement; an own file stuck over the cap on a full or read-only disk
  restarts a compaction at every answer; keying a 100,000-line history (~8 ms) runs on the serve
  thread when a full re-read lands; a torn line also loses the record appended after it; the
  pending-report helper relies on precmd's `emulate`; follow-on candidates are looked up before
  de-duplication. 127
  adapter tests on zsh 5.3.1, 5.8.1 and 5.9; the workspace passes; release benchmark p95 39 µs.

- **2026-11-21** — **M1 Task 6: `forget` and `doctor`.** The binary takes `serve`, `forget` and
  `doctor [--shell-facts <lines>]` (usage and exit 2 otherwise); `doctor::report` prints the
  facts in a fixed order; the zsh adapter's `fuxx-completes` function (only when the name is
  free) passes its copy of `FUXX_PLUGIN_DATA` and, for `doctor`, the shell's facts. A bug the
  tests found (systematic debugging): the command run just before `forget` is reported one
  prompt late, and `record()` wrote it under the forgotten secret, recreating the folder; now
  `record()` checks the key file first and drops records that finished before the forget was
  noticed. `design.md` §7, §16; README (ranking, coexistence, `doctor`, the data). Tests first,
  each seen failing: the report without and with facts, a conflict and second copies, the
  `shglob` warning, counting context files; `forget` as a process (removes, twice, keeps fuxx's
  folder, an I/O error exits 1), usage, `doctor` outside a shell; in zsh the function hands
  `doctor` the facts, `forget` uses the plugin data folder, a user's own `fuxx-completes`
  function is left alone; and in the store a record after `forget`. 126 adapter tests on zsh
  5.3.1, 5.8.1 and 5.9; the workspace passes.

- **2026-11-21** — **M1 Task 5: the zsh adapter.** The double-load guard (the first copy's path;
  another copy stops before defining anything), Warp and zsh-autosuggestions checked at every
  `precmd` (`FUXX_AUTOSUGGESTIONS_FORCE=1` overrides only the latter; a running core is
  stopped), the core started with the adapter's copy of `FUXX_PLUGIN_DATA`, `prev` sent at
  each prompt (the pending command, or empty when the line won't be kept), why the adapter is
  off (`_FUXX_AS_WHY`), which free keys were bound or kept, and `_fuxx_as_facts` for `doctor`.
  `design.md` §8, §9. Tests first; seen failing: a second copy, zsh-autosuggestions present and
  loaded later, Warp even when forced, the plugin data folder, `prev` both ways, the facts.
  Passing from the start (guards of existing behaviour): fzf-style ⌃R and Tab widgets still run,
  a leading-space command makes no record, forcing on beside zsh-autosuggestions. One bug on the
  way (systematic debugging, reproduced by hand): an assoc subscript with a space
  (`_FUXX_AS_KEYS[$1 $2]`) is a "bad pattern" that stopped start-up; now keyed through one
  variable. 123 adapter tests on zsh 5.3.1, 5.8.1 and 5.9.

- **2026-11-21** — **M1 Task 4: context in the session.** `Session::new(data, home)` (tests
  pass temp folders; only `main` reads the environment): `hello` opens the context store and
  keys the history, `ran` records (the secret made on first use) and pairs successors, the new
  `prev` message names the command just run, `suggest` refreshes the store (a `forget` seen
  through the key file), the history file, and compacts its own file once the history loaded,
  then ranks. `History::note_ran`: a `ran` the file already gained adds no session record
  (§5.8's M1 fix). `serve_with`, `HistoryFile::has_loaded`. `design.md` §3, §5.4, §5.5, §5.7,
  §5.8. Tests first, seen failing (missing items): `prev` both ways and empty, the retire rule
  (two commands gained before their `ran`), context surviving a restart, `prev` driving the
  order, `forget` while serving, an unwritable data folder still suggesting, the first full
  read compacting. Benchmark (release): p50 22 µs, p95 29 µs, max 350 µs over 2,000 answers on
  100,000 lines and 50,000 records; history loaded and keyed in 37 ms. The workspace passes,
  the zsh adapter tests included.

- **2026-11-21** — **M1 Task 3: the ranking order.** `core/src/rank.rs`: `Ranking` (tallies, cwd,
  previous command, session successors), `Successors` (consecutive `ran` pairs, at most 10,000),
  the lazy `Ranked` order (tier 1 ran successfully here: follows-previous, then runs here with no
  boost for `cd`/`pushd`/`open`, then newest run here, then recency; tier 2 newest first with
  follows-previous first; tier 3 failed only, last) and M0's `walk` moved here unchanged.
  `History` keys every entry under the secret and finds a key's text without a scan;
  `cd_target::command_word`; `suggestion()` takes a `&Ranking` and walks the ranked order.
  `design.md` §5.4. Tests first, seen failing (missing items): with no context the order is M0's,
  ran-here beats newer, frequency and its ties, no boost for `cd`/`pushd`/`open` (and for
  `builtin cd`, `FOO=1 pushd`), failed-only last (here, elsewhere, mixed), follows-previous
  first (and without a secret), successors bounded, clearing the secret restores M0's order,
  session records keyed, the walk unchanged, and through `suggestion()` a held tier-1 `cd ..`
  gives way to a `cd` down here while a tier-1 `cd` to a missing folder is passed over. The
  workspace passes.

- **2026-11-21** — **M1 Task 2: reading, tallying and compacting the context files.**
  `ContextIndex` (tallies per folder and over all folders: successes, failures, newest time) and
  `ContextStore` (opens a data folder, reads every context file, reads growth and new files at
  each refresh, re-reads in the background when a file is replaced, picks up or drops the
  secret, keeps unwritable records pending) and `compact` (4 MiB cap, trim to 3 MiB, under an
  exclusive try-lock, temp file renamed into place). `design.md` §5.7. Tests first, seen failing
  (missing items): every file read, no secret no records, growth inline, a new file and a new
  secret, a renamed file re-read in the background, junk and torn lines, records without a history
  file, the secret made on first record, a busy append kept pending then written once, compaction
  drops unknown keys and trims the oldest, skips when locked, leaves a clean file alone, two
  writers appending 4,000 records while a compactor loops lose none and tear none, a 16 MiB file
  compacted in the background. 29 context tests; the workspace passes.

- **2026-11-21** — **M1 Task 1: context records.** `core/src/context/`: the data folder
  (`FUXX_PLUGIN_DATA`, then `XDG_DATA_HOME`, then `HOME`), the secret (`blake3` 1.8.7 keyed,
  `pure`; 32 bytes from `/dev/urandom`, hard-linked into place, `0600` in a `0700` folder), the
  record line (`1 <key> <status> <time> <folder>`, folder %-escaped), `append` under a shared
  try-lock, `own_file_id`, `forget`. `design.md` §5.7 and §8. Tests first, seen failing (missing
  items): data-folder order, key file modes and racing creation, keyed hashes, odd folders
  round-trip (spaces, `%`, LF, tab, non-UTF-8), bad lines skipped, the file never holds command
  text, a held lock makes an append busy, `forget` removes only our files. 14 new tests; the
  workspace passes.

- **2026-11-21** — **M0 done; M1 planned.** The user checked M0 in a real terminal, having
  used it since the v0.0.1–v0.0.5 test releases: "close m0, all good". M0 is ✅. The M1 plan is
  written ([`plans/M1 Plan - Context File and Coexistence.md`](plans/M1%20Plan%20-%20Context%20File%20and%20Coexistence.md))
  and waits for the user's review before any code. M1b is planned once M1 is done.

- **2026-11-21** — **Design: frequency ranking and project knowledge (docs only, nothing built;
  saved to pick up later, the user's words: "this session is only to write plan and saved it to
  pick up later").** The user asked whether the ghost text saves the commands run most (except
  `cd` and `open`); it doesn't (newest match wins). Decided: how often a command ran successfully
  in this folder becomes an M1 ranking factor, with no boost for `cd`, `pushd` and `open`. And a
  new milestone **M1b, project knowledge**: the core detects project types from marker files
  (node, uv, cargo, C++, VB.NET, flutter, android, macOS, plus about 25 more) through mapping data
  and six built-in readers (approach 1 of 3), never running anything in the project. Order: this
  folder's history, then the project's commands, then history from other folders (the user's
  "history first", then "agreed" to this folder's history after the builder check found global
  history would almost always hide the project's commands). Limits (AI's calls): 10 ms answers kept,
  detection off the typing path with a 50 ms cap, files over 256 KB skipped, at most 150 KB added
  to the binary. Spec:
  [`superpowers/specs/2026-11-21-project-knowledge-design.md`](superpowers/specs/2026-11-21-project-knowledge-design.md);
  `design.md` §2, §3, §4, §5.4, §5.8, §12, §15 updated. M0 still waits for the user's check; plans
  for M1 and M1b are written when each is reached.

- **2026-11-13** — **A `cd` down into the current folder ranks first (the user's decision, after
  v0.0.4).** The user's report: "now cd always suggest to ../.." (in `~/Kerja`, `../..` is the
  newest `cd` that works from there); then "yes do that". Among `cd`/`pushd` matches, one whose
  target goes down into the current folder (`proj`, `./proj`, `proj/x`, a symlinked folder here)
  ranks above one that works from anywhere (`..`, `~`, absolute, `-`, no argument, `cdpath`-only,
  unchecked forms); a lower one is shown only when no "down here" one matches (`cd .` still gets
  `../..`). Other commands keep their place: the walk ends at the first non-`cd` match. The walk
  (`History::best_with_prefix`) stays within its bounds, which now count held candidates too, and
  returns the held one when it hits the bound or the 50 ms budget. `cd_target::classify` sorts an
  entry into not-`cd`, missing, down here or anywhere. `design.md` §4 and §5.4. Tests first, each
  seen failing: the classifier, the walk (best above a newer held one, newest held without one,
  session first, a plain candidate ends it, bound and budget return the held one, a repeated held
  one checked once), `suggestion()` on a real folder tree (the user's case, `cd .`, no "down here"
  match, `./proj`, bare `cd` and `cd -`, other commands), and in a real zsh the user's case (from
  `~/Kerja` with `cd proj`, `cd Kerja`, `cd ../..` in history, `cd ` suggests `proj` and `cd .`
  suggests `./..`). fuxx-autosuggestions gains `tempfile` as a dev-dependency (already the
  workspace's test crate). 113 adapter tests on zsh 5.3.1, 5.8.1 and 5.9; 235 in the workspace.

- **2026-11-13** — **`cd` target check, fixes after the independent review.** (1) A `cd`/`pushd`
  alias or function (zoxide's `--cmd cd`, a wrapper) or `CDABLE_VARS` reaches folders the core
  can't see: the zsh adapter then leaves the `cdpath` field off, so relative targets not found
  from the cwd are kept (`CDABLE_VARS` is read at the prompt, since the request runs under
  `emulate`). (2) A hung network or iCloud folder could stall the core past the adapter's 2 s
  watchdog: the walk now returns the next candidate unchecked once 50 ms have passed; one hung
  `stat` still can't be interrupted (a worker thread is a later fix). (3) Only a `stat` that
  certainly says no (not found, not a folder, no permission) means missing; any other error keeps
  the candidate. `design.md` §5.4 and §5.5. Tests first, each seen failing: the error mapping, a
  target under a file or a locked folder, the time budget with a slow checker, and in a real zsh a
  wrapped `cd` (function, alias), a `pushd` function and `CDABLE_VARS` keeping `cd fuxx-cli` from
  `Kerja`. 112 adapter tests on zsh 5.3.1, 5.8.1 and 5.9.

- **2026-11-13** — **A `cd` to a folder that isn't there is never suggested (the user's
  decision).** The user's report: in `Kerja`, typing `cd` suggested `fuxx-cli`, which is in
  `Kerja/proj`; then "yes" to never suggesting `cd`/`pushd` to a folder that doesn't exist from the
  current folder, with the next valid match shown instead (folder-aware ranking stays M1, general
  argument checks stay M5). `design.md` §4, §5.4 and §5.5 updated. The core
  (`core/src/cd_target.rs`) decides whether a `cd`/`pushd` target certainly isn't a folder from the
  request's cwd (quoting, `~`, absolute, `.`/`..`, options, `builtin`/`command`/assignments,
  symlinks; anything it can't check is kept); `History::latest_with_prefix_except` walks the
  matches in rank order, checking each distinct candidate once and giving up after 50 rejected.
  `suggestion()` and the session pass the request's cwd, the core's `$HOME` and the `cdpath` the
  zsh adapter now sends as an optional last `suggest` field (old six-field requests still decode,
  `cdpath` unknown: relative targets not found from the cwd are kept). Tests first, each seen
  failing: the resolver (relative, `~`, absolute, quotes and escapes, what's kept, options, two
  arguments, `cdpath` known/unknown, symlinked folder, a file named like the target, logical and
  physical `..`), the walk (next valid match, all invalid, the bound, a repeated candidate), the
  wire field, the session, and in a real zsh the user's case (from `Kerja` `cd ` suggests `proj`,
  from `Kerja/proj` `fuxx-cli`), a folder found through `cdpath`, and the field on the wire. The Tab
  and Esc tests' fixture now makes the folders their `cd` histories go to. 111 adapter tests on zsh
  5.3.1, 5.8.1 and 5.9; 211 in the workspace.

- **2026-11-13** — **Tab and Esc, fix round 2 (re-review).** ⌃D mid-line is a plain forward
  delete, but as a completion widget (`completion:` after compinit) it took the
  hide-until-next-edit path, so "git pu", ⌃B, ⌃D left "git p" with no suggestion. Now ⌃D before
  the end of the line takes the normal edit path (asks again); at the end it still lists and keeps
  the ghost text off. `design.md` §6. Tests (the first seen failing): ⌃D mid-line brings back the
  suggestion and ⌃F accepts it, ⌃D at the end lists with the ghost off, with and without compinit.
  107 adapter tests on zsh 5.3.1, 5.8.1 and 5.9.

- **2026-11-13** — **Tab and Esc, fix round 1 (an independent review with real-zsh pty
  experiments).** (1) The Esc widget dropped any character after Esc that wasn't the tail of a
  sequence: Esc then ↑ typed `[A`, Esc then a paste with a newline lost its bracketing and ran the
  first line, Esc then é lost the é. Now such a character is handed back with `zle -U`; unbound
  sequence tails are still dropped, double Esc still does nothing. (2) Other completion keys
  (Shift-Tab's `reverse-menu-complete`, a ⌃Space bound to `complete-word`) asked again and brought
  the ghost text back mid-menu, so the next Tab accepted it ("cd alpha1/x" where zsh gives
  "cd alpha2/"): every completion widget (`completion:` values, or zsh's builtin completion
  widgets) now leaves the ghost text off until the next edit, as Tab does. (3) Tab accepting while
  an earlier list is on screen clears the list. (4) README: to complete instead of accepting,
  press Esc first. `design.md` §6 updated. Tests first, each seen failing: a key right after Esc
  (↑, paste with a newline, é, a letter), Esc then ⌃→ or another Esc, Shift-Tab and ⌃Space against
  plain zsh with and without compinit, Tab-accept clears a list; the Esc-with-list-and-ghost test
  now gets its list from Tab (⌃D now keeps the ghost text off). The menu tests read the pty
  between keys (an unread pty stalled zsh's next draw and hid the bug). 105 adapter tests on zsh
  5.3.1, 5.8.1 and 5.9.

- **2026-11-13** — **Tab and Esc (the user's decision, after v0.0.2: "hit tab when
  fuxx-suggestions appeared, still showing the list of things inside the dir … a simple esc hit
  to lose them … Make it smooth", then "do 1"; and "ensure we cover all edge cases").** `design.md`
  §2, §6 zsh (Tab, Esc) and §9 changed. Tab: while ghost text shows and the cursor is at the end
  it accepts all of it and no completion runs; otherwise whatever widget Tab is bound to runs as
  before (the wrapper looks at `$KEYS`, so a Tab rebound later is covered at the next `precmd`, and
  Tab is never rebound), and the ghost text stays off until the next edit so a second Tab goes on
  completing (`auto_menu`, `menu-select`); the answer to a request in flight is dropped. Esc:
  `zle -R -c` clears a completion list and the ghost text hides until the next edit; bare `^[` is
  bound in emacs only if free (zsh then waits `KEYTIMEOUT`, 0.4 s by default, after Esc: accepted
  trade-off; unbound sequences such as ⌃→ now do nothing instead of typing their tail; Esc in ⌃R
  ends the search on the line found); in vi mode the `vi-cmd-mode` that a bare Esc runs also
  clears both. Changed tests (the old behaviour is gone): the completion-question test now Tabs
  with the cursor moved left, "a Tab that completes asks again" became "a mid-word Tab completes and
  the ghost stays off until the next edit", and the vi accept tests enter command mode with a
  non-Esc key. The harness reads the pty until it is quiet (one read could leave a completion list
  unread) and reports `$KEYMAP`. 26 new tests (Tab with/without ghost, other plugins' Tab widgets,
  mid-buffer, in flight, after an accept, Tab-list-Tab with `auto_menu` and `menu-select`, literal
  and 995-character suggestions, multi-line, rebinding, unbound Tab; Esc for ghost, list, both,
  in flight, nothing showing, a user's `^[`/`^[^[`, Esc-prefixed keys against plain zsh at
  `KEYTIMEOUT` 1 and 40, bracketed paste, ⌃R, a list longer than the screen with and without
  `ALWAYS_LAST_PROMPT`, vi mode, `bindkey -v` later, re-sourcing, `KEYTIMEOUT` untouched): 101
  adapter tests on zsh 5.3.1, 5.8.1 and 5.9. The list clearing is checked by zsh's erase-below
  sequence (no screen emulator in the harness). Needs the user's look in fuxx's terminal.

- **2026-11-13** — **Fix after v0.0.1 (a bug the user hit; "if some issue come up, we fix it").**
  In zsh 5.9, typing `c` showed the ghost text `d Kerja/`; Tab then asked "do you wish to see all
  151 possibilities?" while the line showed `cd Kerja/` in normal colour, as if typed. Cause
  (reproduced in the pty tests): the wrapper of every edit widget (completion included) kept the
  ghost text on while the original ran; zsh draws the line without `region_highlight` before that
  question, and completion left our entry zero-width (`1 1`), so the text stayed plain after. Now
  the ghost text is taken off while a widget runs and put back, styled, when the buffer didn't
  change (zsh-autosuggestions' way); a changed buffer asks again; an accept or partial widget
  called from another widget still accepts the held suggestion (`design.md` §6 zsh). Tests: the
  completion question never shows the ghost text, `n` brings it back styled, a Tab that completes
  asks again, the ghost text is hidden inside a widget (replaces the "keeps its style while a
  widget redraws" test). 75 adapter tests on zsh 5.3.1, 5.8.1 and 5.9. Needs the user's look in
  fuxx's terminal.

- **2026-11-13** — **v0.0.1 test release, before M3 (the user's call: "just push fuxx-completes so we
  can test it directly by installing it. if some issue come up, we fix it").** M0 merged to `main`.
  Part of M3's packaging pulled forward so fuxx can install it from Settings → Plugins: a
  `fuxx-plugin.conf` at the repo root (design §8) and a GitHub release `v0.0.1` with
  `fuxx-plugin-macos-arm64.zip` (`fuxx-plugin.conf`, `bin/fuxx-completes` built with
  `cargo build --release`, `shell/fuxx-autosuggestions.plugin.zsh`). Apple Silicon only; no x86_64
  or universal zip, no Homebrew, no crates.io (those stay in M3). M0 stays 🟨 until the hand check.

- **2026-11-13** — M0 started. Wire format 1 (protocol types and line encoding in
  `core/src/protocol.rs`; field-separated, JSON deferred; `design.md` §5.5 and §17 Q1 updated).
  zsh history parser (`core/src/zsh_history.rs`: metafied bytes, EXTENDED_HISTORY, multi-line).
  History index (`core/src/history.rs`) and `suggestion()` in `fuxx-autosuggestions`.
  History file re-reading (`core/src/history_file.rs`: growth read inline, any other change a background full read).
  The `fuxx-completes serve` binary (`cli/`: session, serve loop, tested over real pipes).
  zsh adapter core (`adapters/zsh/fuxx-autosuggestions.plugin.zsh`, pty harness `cli/tests/common/mod.rs`): serve on named pipes, async ghost text, hung/dead core handled.
  zsh accept, partial-accept and clear widgets; End and ⌥→ bound only if free; vi mode (`emacs`/`viins` keymaps only).
  zsh command reports (`zshaddhistory`/`preexec` hooks, one prompt late by the user's decision, filters per §5.8) and the history re-read tested end to end.
  zsh matrix on 5.3.1, 5.8.1 and 5.9 (`scripts/build-zsh.sh`, `scripts/test-zsh-matrix.sh`); 5.3.x has no working close-on-exec, so the adapter uses plain fds there.
  M0 is built and reviewed (Tasks 1–9); `cargo test --workspace` and the zsh matrix pass. The user
  decided reports go one prompt later (option b, §5.8). The AI's calls along the way: wire format 1
  instead of JSON; named pipes in a private temp folder and a ready handshake; a dead or hung core
  is handled (2 s) without signalling a stale pid; the 1000-character and `0x1F`/`0x1E` buffer
  limits; the binary found through `FUXX_COMPLETES_BIN`, then `PATH`; history growth trusted only
  on the same device and inode, else a full re-read; End and ⌥→ bound only if free, in `emacs` and
  `viins` only; plain fds on zsh 5.3.x; the matrix script with zsh 5.3.1, 5.8.1 and 5.9. Waiting
  for the user's check in real terminals (Task 10 Step 1); M0 is not marked done until then.
  Final review Critical fixed: each widget wrap now uses fresh generation names
  (`fuxx-as-orig-N-<widget>`), so a zsh-syntax-highlighting-style plugin that wraps our wrappers
  (loaded late, or after `source ~/.zshrc`) no longer makes every key recurse until FUNCNEST;
  tested with a fake highlighting plugin loaded after, re-sourced and loaded before ours.
  Re-review fixes: a widget that still calls our wrapper is not wrapped again (a plugin that
  re-wraps at every prompt no longer grows the chain or the widget table; 20 generations at most),
  other plugins' `*-orig-*` saved widgets are left alone, and only the outermost of our wrappers
  acts when another plugin nests them (partial accept no longer doubles the suggestion, one
  request per key).
  Second re-review fixes: the "only the outermost acts" guard is keyed on the widget, so a user
  widget that calls `zle end-of-line` or `zle forward-word` accepts again; a widget set back to a
  definition we wrapped before reuses that generation, so re-sourcing an oh-my-zsh rc no longer
  reaches the 20-generation cap and silently ends suggestions; the "still calls us" memo includes
  the function's body.
  Third re-review fixes: the cap of 20 now counts only our generations of a widget that some
  widget still runs, not every wrap ever made, so oh-my-zsh plus a highlighter that wraps under a
  fresh name on each load can be re-sourced any number of times (25 tested) and keep ghost text; a
  plugin that hides that it calls us is still stopped at 20. Partial accept clears the ghost text
  before running the original (an original that calls another partial widget no longer appends
  the suggestion twice), and a builtin widget re-aliased with `zle -A` is no longer undone by
  reusing an old generation.
  Final review fixes: widgets are wrapped at every prompt (not only the first); writes to the core never block (non-blocking fd on zsh 5.9+, byte count below); the core starts from `/` and not niced; the spare `resp` reader stays until `ready`; the core drops suggestions over 1000 characters; the history stamp is taken after the read; README try-it copy is private and removed; build-zsh.sh never keeps a half install; design wording updated.

- **2026-11-13** — fuxx gained plugins (fuxx note 76, approved by the user). fuxx no longer links
  these crates; it installs fuxx-completes as a plugin. `design.md` §2, §4, §8, §9 and §15 updated:
  the plugin manifest and release zips (M3), the context file under `FUXX_PLUGIN_DATA` and a
  double-load guard (M1). fuxx drawing the menu itself is later.

- **2026-11-09** — Design revised after a builder check. The user decided context data lives in a
  small local file (folder, exit code, time per command; keyed hash, no command text; `forget`
  wipes it). AI's calls written in: bash explicit keys wait at most 50 ms (`sync` requests), history
  semantics (only commands the shell saved; file re-read on change), host embedding over OSC 7701
  with `FUXX_COMPLETES_HOST=1`, fish reports through `fuxx-completes report`, shell state, versions
  (zsh 5.3+, bash 4.0+ or ble.sh on 3.0+, fish 3.0+) and a test matrix, End and ⌥→ bound only if
  free, vi mode. Facts fixed: Warp is a stay-off case; Fig's history; inshellisense and carapace are
  multi-shell; most Fig generators won't convert; fish merges completions; atuin; zsh-autocomplete
  detection; ble.sh on bash 3.0+. Milestones moved: the context file into M1, the host channel into
  M2, shell state into M5. No code.

- **2026-11-09** — Repo created: design, README, agent instructions, empty Cargo workspace,
  licenses. No code.
