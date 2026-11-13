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
