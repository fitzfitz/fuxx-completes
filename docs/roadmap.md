# Roadmap

The milestones from [`design.md`](design.md) §15. One at a time, in order. Status: ⬜ not started ·
🟨 in progress · ✅ done.

Building starts after fuxx's v0.5.0 release.

**Current milestone: M2** (M1d done 2026-11-22; M2 approved 2026-11-23, being built on `m2-bash-fish-host`; its test build will be v0.0.13). fuxx's Settings for plugins shipped in fuxx v0.12.6; `fuxx-plugin.conf` declares the list's four settings since v0.0.12.

| | Milestone | Status |
| --- | --- | --- |
| M0 | Core + history + zsh adapter | ✅ done 2026-11-21 (the user's check: "close m0, all good") |
| M1 | Autosuggestions: context file and coexistence (now with frequency ranking, no boost for `cd`/`pushd`/`open`) | ✅ done 2026-11-22 ([plan](plans/M1%20Plan%20-%20Context%20File%20and%20Coexistence.md); the user's check on v0.0.7) |
| M1b | Project knowledge (added 2026-11-21; [design](superpowers/specs/2026-11-21-project-knowledge-design.md)) | ✅ done 2026-11-22 ([plan](plans/M1b%20Plan%20-%20Project%20Knowledge.md); the user's check on v0.0.8) |
| M1c | Suggestion list (added 2026-11-22; [design](superpowers/specs/2026-11-22-suggestion-list-design.md)) | ✅ done 2026-11-22 ([plan](plans/M1c%20Plan%20-%20Suggestion%20List.md); the user's check on v0.0.10) |
| M1d | Safety and polish (added 2026-11-22; [design](superpowers/specs/2026-11-22-safety-and-polish-design.md)): secret filter, dangerous-command guard, repo/worktree ranking, fix after failure, `why` and `doctor --report` | ✅ done 2026-11-22 ([plan](plans/M1d%20Plan%20-%20Safety%20and%20Polish.md); the user's call on v0.0.11) |
| M2 | bash and fish adapters, and the host channel (autosuggestions) | 🟨 approved, being built ([plan](plans/M2%20Plan%20-%20bash%2C%20fish%20and%20the%20Host%20Channel.md)) |
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

- **2026-11-23** — **M2 Task 6: the host channel (zsh, bash).** `core/src/host.rs` encodes and
  decodes OSC 7701 messages and the accept; with `FUXX_COMPLETES_HOST=1` (and no tmux/screen) the
  zsh plugin forwards `hello`, `buffer` as you type, `clear`, `ran` and `prev` to the terminal
  and starts no core; bash does the same at its prompts and explicit keys; both bind `ESC [ 7701 ~`
  where free and apply only the last line's id. The number was re-checked against 13 terminals'
  docs and source: no clash (mintty uses other 77xx numbers). Found: bash's `read` waits until
  the terminal has read the shell's output (`TCSADRAIN`), so the tests' fake host keeps reading
  while it waits; a real terminal always does. `doctor` shows host mode. Tests seen failing
  first; zsh and bash matrices pass. `design.md` §5.5, §6, §7; `README.md`.

- **2026-11-23** — **M2 Task 5: fish reports through `fuxx-completes report`.** The binary gains
  `report` (one record, no history load, never prints; a busy lock is retried for 100 ms; the
  file is trimmed past its 4 MiB cap, U2 = F1) and `doctor` shows fish's `reporting` fact.
  `adapters/fish/fuxx-autosuggestions.fish` reports each command fish keeps from
  `fish_postexec`, in the background and disowned (the prompt never waits, no job lines), with
  the folder it started in; nothing for a leading space, private mode, `fish_history` set empty,
  a `fish_should_add_to_history` that says no (4.0+), or Warp. Tests seen failing first; 7 fish
  tests pass on fish 4.9.3 (3.x doesn't build here). `design.md` §5.5, §5.7, §5.8, §6, §7;
  `adapters/fish/README.md`, `README.md`.

- **2026-11-23** — **M2 Task 4: ble.sh shows our suggestions.** Under ble.sh 0.4+ the bash adapter
  registers a source first in ble.sh's auto-complete list, so ble.sh's grey text is our ranking
  (a `git stash` that worked here beats a newer `git status`); it polls the core every 5 ms for
  at most 50 ms inside ble.sh's idle task and gives way the moment a key arrives; beside atuin's
  ble.sh source it steps aside. Found: ble.sh loads bash's history after the first prompt (a
  baseline fix keeps that from being reported as a command), and on bash 4.0 ble.sh showed no
  grey text even of its own, so the ble.sh tests run on bash 5.3 as planned. 23 bash tests pass
  on 4.0 and 5.3 (ble.sh ones on 5.3). `design.md` §3, §6; `adapters/bash/README.md`.

- **2026-11-23** — **M2 Task 3: the bash adapter.** `adapters/bash/fuxx-autosuggestions.bash`:
  → and End at the end of the line insert the best suggestion (the fix on an empty line after a
  failure), waiting at most 50 ms; mid-line they move as before; a user's binding is kept. A
  `PROMPT_COMMAND` entry, put first, reports what bash saved (`ran`, `prev`) and hands the
  command's `$?` on. Named pipes as in zsh, counted writes with a ping so a healthy core is never
  stopped, no command substitution on the prompt path. Found on the way, and handled: bash before
  4.4 redraws the prompt on a new line for every `bind -x` key (so → stays readline's there and
  End inserts), bash 4.0 doesn't draw the first prompt after a command substitution in
  `PROMPT_COMMAND`. 18 tests, seen failing first, pass on bash 4.0 and 5.3 (3.2 for staying off).
  `design.md` §5.5, §5.8, §6, §7, §9; `adapters/bash/README.md`, `README.md`.

- **2026-11-23** — **Chore: fuxx's Settings for the list.** fuxx v0.12.6 shipped declared plugin
  settings (its M11au), so `fuxx-plugin.conf` now carries the four lines of the M1c spec
  (`setting.list`, `list_empty`, `list_delay`, `list_size`); the adapter already reads
  `FUXX_PLUGIN_SETTING_LIST*` (M1c). The user: "go". Test build v0.0.12, built from `main` in a
  separate worktree while M2 is built on its branch; M2's test build becomes v0.0.13.

- **2026-11-23** — **M2 Task 2: the core reads bash's history.** `core/src/bash_history.rs` reads
  the file as bash does (timestamps dropped, lines between two timestamps one multi-line entry,
  plain files line by line, CRLF trimmed); `hello … bash …` serves it, `why --format bash` explains
  a line from it, and `doctor`'s hint names zsh and bash. Measured: 2,000 `sync` requests on a
  100,000-entry timestamped bash history through the real serve loop, round trip p50 9 µs, p95
  16 µs, max 0.43 ms (release). `design.md` §5.1, §7. Tests seen failing first.

- **2026-11-23** — **M2 Task 1: test shells and one pty harness.** `scripts/build-bash.sh`,
  `build-fish.sh` and `get-blesh.sh` put bash 4.0 and 5.3, fish 4.9.3 and ble.sh 0.4.0-devel3
  into `target/` (pinned SHA-256s); `scripts/test-bash-matrix.sh` and `test-fish-matrix.sh` run
  the adapter tests per version. The zsh harness now sits on one pty core shared with new bash
  and fish harnesses (temp HOME, clean environment, empty `INPUTRC`). **fish 3.0.2 and 3.7.1
  don't build here**: their generated `config.h` defines names (`__sentinel`, `__fallthrough__`)
  that this Mac's libc++ headers use; not patched, per the user's rule, so fish runs on 4.9.3 only
  until the user decides (`design.md` §14). Found: bash before 4.3 can't `bind -x` a key longer
  than two bytes (a macro to a two-key sequence works), and fish 4 waits for the terminal to
  answer a device query before its first prompt.

- **2026-11-23** — **M2: the user's four decisions.** The user: "go", approving every AI's pick
  of the plan: **U1 = B2**, feed ble.sh's own grey suggestions (AGENTS.md rule 6 now allows that
  second bounded wait: 5 ms checks while ble.sh is idle, at most 50 ms, given up the moment a key
  arrives); **U2 = F1**, `report` keeps only the 4 MiB cap, and `design.md` §5.7 states the limit;
  **U3 = A1**, the bash key inserts the best suggestion (the fix after a failure comes free), no
  list in bash; **U4 = C2**, every test shell built into `target/`, a version that won't build is
  reported. Recorded in the plan, `design.md` §3, §5.7, §5.8 (our `PROMPT_COMMAND` entry goes
  first), §6 bash, §9, §14 and §17 q2. M2 is being built.

- **2026-11-22** — **M2 plan written.** [`plans/M2 Plan - bash, fish and the Host Channel.md`](plans/M2%20Plan%20-%20bash%2C%20fish%20and%20the%20Host%20Channel.md):
  seven tasks (test shells and one pty harness; bash's history format; the bash adapter with
  explicit keys and `sync` requests; ble.sh; fish and `fuxx-completes report`; the host channel in
  zsh and bash; measuring, one whole-branch review, one fix pass, a test release). Checked for it:
  bash `CHANGES` and `histfile.c`, the ble.sh manual and source, atuin's ble.sh hook, fish's docs.
  Findings: ble.sh's auto-complete needs bash 4.0+, so it can't rescue macOS's bash 3.2 (design
  §3, §6 hoped it would); ble.sh can take our suggestions only through an interface its manual
  doesn't document (atuin uses it); our `PROMPT_COMMAND` entry must run first, not last, to see
  the command's `$?`; nothing would ever prune fish's context file by history. **Waiting for the
  user's four decisions** (the ble.sh path, fish pruning, the list in bash, the test shells); the
  plan is written with the AI's picks.

- **2026-11-22** — **M1d done.** The user: "close m1d then plan M2". M1d is ✅ on test build
  v0.0.11 and merged to `main`; the user had not tried v0.0.11 in fuxx before closing it, so
  anything found there later is fixed as a chore. Next: M2's plan.

- **2026-11-22** — **M1d: the user's two calls.** "go with your picks": `cli/tests/size.rs` now
  holds the whole release binary under one limit of 768 KB (786,432 bytes; M1d's is 702,224), in
  place of M1b's 150 KB on top of M1; and `why` not saying whether a line followed the previous
  command is accepted (spec §4.5 notes it). Test build v0.0.11 next, for the user's check.

- **2026-11-22** — **M1d whole-branch review and its fix pass.** One fresh reviewer (Opus): no
  Critical, two Important; two more regraded to Important by their effect. Fixed, each with a
  test seen failing first: the secret-name rule hid everyday commands (`--max-tokens`,
  `TOKENIZERS_PARALLELISM`, `--secret-id`, `--password-stdin`, `--no-password`): names now match
  whole segments, and a `name=value` inside a value counts (`--from-literal=password=x`);
  `git checkout .` (without `--`) was not guarded; an answer could pass over any number of
  hidden lines (now at most 1,000); the repo tier looked every folder of a big monorepo up again
  after each command (membership now remembered per repo). `design.md` §5.4. Deferred (minor):
  `/bin/rm`, `noglob`/`time`/`env`/`nohup`/`exec` prefixes and `sudo -iu` clusters aren't seen
  as dangerous; submodule subfolders get no repo tier; `why` loses shell quoting (quote the
  whole line); the `--report` warning names only the data folder's path; `repo_order` runs twice
  per answer. **Waiting for the user's check**; the user's two calls are in the entry above.

- **2026-11-22** — **M1d measured.** `cli/tests/latency.rs` gains the repo tier's worst case and
  the fix: suggestions p95 140 µs (history), 1.7 ms (every request merging a 200-folder repo),
  fix p95 5.1 ms, list p95 44 µs; all within 10 ms. The release binary is 702,224 bytes (+66 KB
  over M1c), past `size.rs`'s 656,400 limit (M1b's budget); left for the user to decide.
  `design.md` §3.

- **2026-11-22** — **M1d Task 5: `why` and `doctor --report`.** `fuxx-completes why <line>`
  (the shell function passes `--cwd` and `--histfile`) explains the step a line ranks in, its
  runs here / in this repo / anywhere, its project and file, and its flags, writing nothing;
  `doctor --report` fences the report with the OS, machine and terminal, under a check-first
  line. Project detection now remembers where each candidate came from. `design.md` §7; README.
  Tests first, seen failing; 170 adapter tests on zsh 5.3.1, 5.8.1 and 5.9.

- **2026-11-22** — **M1d Task 4: a fix after a failed command.** `prev` gains the exit status
  (`prev␟cmd␟status`); the zsh plugin asks on the empty line after a failure even with the list
  off; the core answers with a fix as ghost text (and item 1 of the empty-line list): the
  folder's package manager, a command-word typo (only after 127, command not found), or the
  closest successful line here or in the repo; never for a line that worked before, never a
  dangerous or secret-looking line, never run. `design.md` §5.4, §5.5, §6; README. Tests first,
  seen failing; 169 adapter tests on zsh 5.3.1, 5.8.1 and 5.9.

- **2026-11-22** — **M1d Task 3: repo and worktree ranking.** `core/src/repo.rs`: a folder's
  repo key (the git folder all its worktrees share), cached for 256 folders and 60 s; the
  session merges the records of the repo's other folders, and the ranking puts **this repo's
  history** between this folder's and the project's commands (ghost text, the list, the
  empty-line list). Odd layouts (bare repos, submodules, a removed worktree, `.git` at home,
  unreadable files) have no or their own key. `design.md` §5.4. Tests first, seen failing.

- **2026-11-22** — **M1d Task 2: the dangerous-command guard.** `safety::danger_reach` over a
  const table (spec §4.2's list) and the first simple command's words with byte offsets
  (`cd_target::command_words`); a dangerous line gets no lift of any kind and is shown only
  once the typed text reaches its dangerous part (`rm -`, `git push -`, `git reset --h`,
  `kubectl delete`). `design.md` §5.4. Tests first, seen failing; one older adapter test used
  a recursive `rm` as a plain example and now uses `rm build.log`.

- **2026-11-22** — **M1d Task 1: the secret filter.** `core/src/safety.rs` `looks_secret`: known
  token shapes, `Authorization:`/`Bearer` values, URL passwords, secret-named values and long
  random-looking words; such a line is never ghost text or a list item, and it leaves the
  candidates before the walk so it never uses up the walk's bounds. Everyday lines (snake_case
  test names, branch names, UUIDs, SHAs, digests, paths, `Bearer $TOKEN`) stay. `design.md` §5.4,
  §15. Tests first, seen failing.

- **2026-11-22** — **M1d plan written.** [`plans/M1d Plan - Safety and Polish.md`](plans/M1d%20Plan%20-%20Safety%20and%20Polish.md):
  five tasks (secret filter, dangerous guard, repo/worktree tier, fix after failure, `why` and
  `doctor --report`), then measuring, one whole-branch review and one fix pass. Built in the
  session, the user's standing choice.

- **2026-11-22** — **M1d designed and approved.** From three analyst agents' reports the user
  picked five items ("let's go do #3–#7"), chose the fix-after-failure shape (A: grey text on the
  empty line), and approved the design: [`superpowers/specs/2026-11-22-safety-and-polish-design.md`](superpowers/specs/2026-11-22-safety-and-polish-design.md).
  A builder check's five questions were settled as AI's calls in the spec.

- **2026-11-22** — **M1c done.** The user checked v0.0.10 in fuxx: "all good", then "go ahead
  close it". M1c is ✅ and merged to `main`. The fuxx Settings piece (declared plugin settings)
  waits: another agent is working in fuxx right now, and the user doesn't want the flow disturbed.

- **2026-11-22** — **v0.0.10 test release (the user: "deploy it directly").** The list's new
  look, tagged on the M1c branch like v0.0.9; `main` untouched.

- **2026-11-22** — **M1c Task 7: the list's look.** After v0.0.9 the user: "the list appears not
  really good … add like title … better in UI and UX"; chose to fix it in fuxx-completes, picked
  look A of three browser mockups, approved the design ("Approved and deploy it directly"). A
  dim title, blue numbers, commands at normal brightness, `▸` and a grey background on the
  selected row, a dim key hint; a short window drops the hint, then the title. Spec §9,
  `design.md` §6. Tests first, seen failing: the title, hint and colours; the short window; the
  existing list tests updated for the layout. 164 adapter tests on zsh 5.3.1, 5.8.1 and 5.9.

- **2026-11-22** — **v0.0.9 test release (the user: "go deploy and release it then"), before the
  M1c check.** The suggestion list, tagged on the M1c branch like v0.0.6–v0.0.8; `main` untouched.
  Binary 635,744 bytes.

- **2026-11-22** — **M1c whole-branch review and its fix pass.** One fresh reviewer (Opus): one
  Critical and six Important. Fixed, each with a test seen failing first: (critical) the first
  key after the list opened was read in the old keymap, so ↓, ⌥n and `¡` failed on the first press
  (the harness's state dump had been using up that read; a screen-wait helper now tests it);
  ⌃C left a stale list and ghost text; Esc on ⌃→ typed its tail with the list open; a cursor move
  brought back a hidden ghost text (vi `Esc h l A`) and could open the list over a completion
  listing; the list could open over ⌃R; rows were cut by characters, not columns (CJK wrapped);
  hostile user variables (`typeset -r cand`, `on='1+'`) printed at the prompt; and (regraded from
  Minor, it printed at the prompt) a `bindkey -s` string on ↑/Enter/Tab. `design.md` §6. 162
  adapter tests on zsh 5.3.1, 5.8.1 and 5.9; the workspace passes. Deferred (minor): unwrapped
  widgets (`yank`, `_*` completion widgets) leave the list up; keymap-change hooks see
  `fuxx-as-list`; item 1 can differ from the ghost text if history changed during the pause; the
  `LINES-3` cap ignores a multi-line prompt; a stream of non-`suggest` lines could delay a list;
  `£`/`¡` typed on UK/Spanish keyboards pick while the list shows.

- **2026-11-22** — **M1c Task 5: doctor and the list's speed.** `doctor` shows the list
  settings and where each came from (`list         on (fuxx Settings)`, `list delay   3 s
  (default)`, …). `cli/tests/latency.rs` times the list of 5 on the 100,000-line history (release):
  p50 4 µs, p95 26 µs. `design.md` §3, §7. Test first, seen failing: the doctor lines.

- **2026-11-22** — **M1c Task 4: the list keys.** A keymap of our own while the list shows (a
  fresh copy of the one it opened over): ↓/↑ (the ghost text follows), ⌥1–⌥5 (also `¡ ™ £ ¢ ∞`),
  Enter and Tab insert the highlighted item and run nothing, Esc closes and then does what Esc did
  (vi command mode), ↑/Enter/Tab with nothing highlighted do what they did. `design.md` §6,
  README. Tests first; seen failing: ↓ and the ghost, ↑ inside, ⌥2, `¡`, Enter and Tab with a
  highlight, Enter and Tab without, Esc, a pick past the list, vi mode, the keymap restored;
  passing from the start: ↑ as history, `£` typed with no list. 151 adapter tests on zsh 5.3.1,
  5.8.1 and 5.9.

- **2026-11-22** — **M1c Task 3: the list under the line.** The zsh plugin resolves the list
  settings (fuxx's, then the shell's, then the defaults), sends `listcfg`, asks on an empty line
  and after keys that only move, closes the list on every wrapped key, drops late lists, and
  draws the rows (cut to the width) with highlight entries it can remove exactly. The harness
  turns the list off by default (list tests turn it on), gains a window size, and takes a
  cursor-down as a line end. `design.md` §6. Tests first, seen failing: the list after a pause,
  none while typing, on an empty line, settings off, a late list not drawn, typing closes it, a
  narrow terminal, another plugin's highlight kept, fuxx's setting winning; passing from the
  start: no list after a Tab completion. One regression found on the way: a parenthesised pattern
  broke loading under `setopt shglob`; plain comparisons now. 137 adapter tests on zsh 5.3.1,
  5.8.1 and 5.9.

- **2026-11-22** — **M1c Task 2: the wire and the timer.** `listcfg` and `list` messages;
  an empty-buffer `suggest`; `Session` arms one pending list per newest `suggest` (`ran`, `bye`,
  size 0 disarm it) with `next_deadline`/`tick`; `serve_with` reads lines on a thread and waits
  with `recv_timeout`. `design.md` §5.5. Tests first, seen failing (missing variants): the
  messages both ways (multi-line items), the empty `suggest`, the list due after the delay, a
  newer `suggest` re-arming it, no list without config or with size 0 or after `ran`, the empty
  line only when enabled, and the real binary pushing a list after the pause and not for an older
  request.

- **2026-11-22** — **M1c Task 1: the list in the core.** `fuxx_autosuggestions::suggestion_list`
  (item 1 = the ghost text's line, then the three-step sequence; on an empty line, follows,
  this folder, project, newest), sharing `best_entry` with `suggestion`; `rank::follows_of` public.
  `design.md` §5.4. Tests first, seen failing (missing function): the ghost first, capped and
  distinct, missing `cd`s and the buffer skipped, the three steps, the empty-line order, empty.

- **2026-11-22** — **M1c planned.** [`plans/M1c Plan - Suggestion List.md`](plans/M1c%20Plan%20-%20Suggestion%20List.md):
  6 tasks (the list in the core; the wire and the core's pause timer; drawing under the line;
  the list keys; doctor and latency; review and the user's check).

- **2026-11-22** — **M1b done.** The user installed v0.0.8 in fuxx and checked it: "0.0.8 works".
  M1b is ✅ and merged to `main`. Next: the M1c plan (the suggestion list).

- **2026-11-22** — **v0.0.8 test release (the user: "yes go ahead"), before the M1b check.** M1b's
  project commands, tagged on the M1b branch like v0.0.6/v0.0.7; `main` untouched. Binary 602,496
  bytes.

- **2026-11-22** — **Design: the suggestion list (M1c), approved by the user.** Up to 5
  suggestions under the line after a pause (3 s), on an empty line too; ⌥1–⌥5 or ↑/↓ then
  Enter/Tab put one into the line (never run); Esc closes; settings as shell variables and, in
  fuxx, Settings → Plugins (a fuxx plugin-format change, planned in fuxx's repo). Spec:
  [`superpowers/specs/2026-11-22-suggestion-list-design.md`](superpowers/specs/2026-11-22-suggestion-list-design.md);
  `design.md` §2, §4, §15. A builder check asked five questions: four settled as AI's calls in the
  spec (↑ stays history with nothing highlighted, any key restarts the pause, fuxx's Settings win
  when changed there, M1c after M1b's check), one answered by the user (the empty line: "yes
  agreed"). Planned after the user's check of M1b.

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
