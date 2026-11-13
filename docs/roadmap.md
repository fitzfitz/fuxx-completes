# Roadmap

The milestones from [`design.md`](design.md) §15. One at a time, in order. Status: ⬜ not started ·
🟨 in progress · ✅ done.

Building starts after fuxx's v0.5.0 release.

**Current milestone: M0** (built; waiting for the user's check in a real terminal).

| | Milestone | Status |
| --- | --- | --- |
| M0 | Core + history + zsh adapter | 🟨 built, waiting for the user's check in a real terminal |
| M1 | Autosuggestions: context file and coexistence | ⬜ |
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
