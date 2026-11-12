# Roadmap

The milestones from [`design.md`](design.md) §15. One at a time, in order. Status: ⬜ not started ·
🟨 in progress · ✅ done.

Building starts after fuxx's v0.5.0 release.

**Current milestone: M0** (in progress).

| | Milestone | Status |
| --- | --- | --- |
| M0 | Core + history + zsh adapter | 🟨 |
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
