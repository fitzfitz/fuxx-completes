# M1c Suggestion List Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Written 2026-11-22 after the user approved the design and said "yes" to writing this plan.
Decisions marked **AI's call** are open to change; each lands in `docs/design.md` in the task named.

**Goal:** After a pause (3 s by default), up to 5 suggestions under the zsh command line, on an
empty line too; ⌥1–⌥5, or ↓/↑ then Enter or Tab, put one into the line without running it; Esc
closes it; configurable through shell variables (and, once fuxx has them, fuxx's plugin settings).

**Architecture:** The core times the pause: the serve loop reads requests on a thread and waits
on a channel with a deadline, so after the last `suggest` it pushes a `list` message. The list is
the same three-step order as the ghost text, `N` whole lines. The zsh plugin sends the settings
once (`listcfg`), draws the list in `POSTDISPLAY` under the ghost text, and owns the list keys in
a keymap of its own only while the list shows.

**Tech Stack:** Rust 2021 (std threads and `mpsc`, no new dependency); zsh 5.3+ (ZLE, `bindkey -N`,
`zle -K`, `zle -F`).

**Spec:** `docs/superpowers/specs/2026-11-22-suggestion-list-design.md`; `docs/design.md` §3,
§5.4, §5.5, §6, §7, §8. Rules: `AGENTS.md`.

## Decisions this plan settles

| Question | **AI's call** | Why |
| --- | --- | --- |
| Settings on the wire | A new no-reply message `listcfg` (size 0–5, 0 = off; delay in ms; empty 0/1) sent once after `hello` | The `suggest` field list already ends with an optional `cdpath` the adapter sometimes leaves off, so another optional field can't follow it; one message per shell is simpler |
| The pushed list | `list` (id, then one field per item, newlines as `RS`); sent only when there is at least one item | Same wire format 1 rules |
| Old peers | An M1b core ignores `listcfg` (unknown kind), so no list; an M1b adapter never sends it | No version bump |
| Empty line | `suggest` with an empty buffer is now sent (only when the list and the empty-line setting are on); the core answers "no suggestion" and arms the list | Spec §4.1 |
| What restarts the pause | Every wrapped widget bumps a key counter and closes the list; a widget that left the buffer unchanged sends a fresh `suggest` (when the list is on), which re-arms the core's timer; a `list` is drawn only if its id is the last answered one and no key came since | Spec §4.3 "any key" |
| fuxx's plugin settings | The adapter reads `FUXX_PLUGIN_SETTING_LIST`, `_LIST_EMPTY`, `_LIST_DELAY`, `_LIST_SIZE` at load (copied like `FUXX_PLUGIN_DATA`); a non-empty one wins over the shell variable. `fuxx-plugin.conf` gets no `setting.` lines in M1c: fuxx doesn't parse them yet, and they are added with fuxx's milestone | Spec §4.4 precedence; never ship a manifest fuxx might reject |
| Values | on/off accept `on off 1 0 true false yes no`; delay in seconds, decimals allowed, clamped to 0.1–60; size clamped to 1–5; anything unreadable → the default | Lenient, never an error at the prompt |
| Drawing | `POSTDISPLAY` = ghost text + `\n` + rows `"  N  <item>"`, each cut to `COLUMNS-1` with `…`; at most `LINES-3` rows; region highlights: the ghost (as today), the rows dim (`fg=8`), the highlighted row `standout`; below zsh 5.9 the exact strings are kept in an array | Spec §4.2 |
| Keymap | On open: `bindkey -N fuxx-as-list <current keymap>` (a fresh copy each time), bind the list keys, `zle -K fuxx-as-list`; on close: `zle -K` back. ↑/↓/Enter/Tab outside the list run the widget the key had in the previous keymap | No binding outside the list changes |
| ⌥ keys | `^[1`…`^[5` and `¡ ™ £ ¢ ∞`; a pick beyond the list's length does nothing | Spec §4.3 |

## Global Constraints

- A keystroke never waits on the core; the list arrives through `zle -F` like the ghost text.
- Suggestion answers stay under **10 ms at p95** (design §3); building a list of 5 on a
  100,000-line history stays under 10 ms too (the pushed list never blocks typing either way).
- A pick is put into the line and **never run**; with nothing highlighted, Enter runs the line,
  ↑ is history, Tab is today's Tab.
- Defaults: list on, on an empty line too, **3 s**, **5** items.
- No new dependency; nothing new written to disk or sent anywhere.
- zsh 5.3.1, 5.8.1 and 5.9 (`scripts/test-zsh-matrix.sh`); every adapter function starts with
  `emulate -L zsh`; nothing prints at startup or at the prompt.
- Per-task gate: `cargo test --workspace`, both `cargo clippy` runs, `cargo fmt --check`; adapter
  tasks also the matrix. Docs with every task. Branch `m1c-suggestion-list`.

## Review Focus

1. **A user pausing mid-edit and then pressing ↑, Enter or Tab** expecting their usual action:
   with nothing highlighted, nothing about those keys changes. Tests in Task 4.
2. **A late list after the user moved on** (typed, moved the cursor, ran the line): never drawn.
   Tests in Tasks 2–3.
3. **A tiny or narrow terminal** (10 columns, 6 rows) and very long commands: rows cut, fewer
   items, no wrapped garbage. Test in Task 3.
4. **Another plugin's `region_highlight`** (zsh-syntax-highlighting) and zsh's own completion
   list: ours never removes theirs; no list while a completion list or ⌃R is up. Tests in Task 3.
5. **vi mode:** the list works in `viins`; Esc closes the list and still enters `vicmd`. Test in
   Task 4.

---

## File map

| File | Change |
| --- | --- |
| `fuxx-autosuggestions/src/lib.rs` | `suggestion_list` |
| `core/src/rank.rs` | `follows_of` public |
| `core/src/protocol.rs` | `Request::ListConfig`, `Response::List`; empty-buffer `suggest` |
| `cli/src/session.rs` | list config, the pending list, `next_deadline`, `tick` |
| `cli/src/serve.rs` | reader thread + `recv_timeout` |
| `adapters/zsh/fuxx-autosuggestions.plugin.zsh` | settings, `listcfg`, empty-line and re-requests, the list, its keymap and widgets, facts |
| `cli/src/doctor.rs` | the list lines |
| `cli/tests/zsh_adapter.rs`, `cli/tests/serve.rs`, `cli/tests/latency.rs` | tests |
| `docs/design.md`, `docs/roadmap.md`, `README.md` | per task |

---

### Task 1: The list itself (core)

**Interfaces — Produces:** `pub fn suggestion_list<'a>(history: &'a History, buffer: &str, place: &Place, ranking: &Ranking, projects: &'a [String], max: usize) -> Vec<&'a str>` — whole command lines; for a non-empty buffer item 1 is the line `suggestion()` would complete to, then the same candidate sequence (this folder's history, project commands longer than the buffer, the rest) minus `cd`s to missing folders, minus the buffer itself, distinct, up to `max`, within `CHECK_BUDGET`; for an empty buffer the order is `rank::follows_of(…, "", …)`, then this folder's history, the project's commands, then the newest history. `rank::follows_of` becomes `pub`.

- [ ] **Step 1: Failing tests** (fuxx-autosuggestions): `list_starts_with_the_ghost_text` (history `["git status","git stash","git push"]`, buffer `git s` → `["git stash","git status"]` with `git stash` first because it is newest); `list_is_capped_and_distinct` (8 matches, max 5 → 5 distinct); `list_skips_missing_cd_targets_and_the_buffer_itself`; `list_follows_the_three_steps` (record here for `cargo test --release`, project `cargo build`, other history `cargo tree` → that order for `cargo `); `empty_line_list_order` (successors `git add .`→`git commit`, previous `git add .`, a record here, a project command, other history → follows first, then here, then project, then newest); `empty_line_without_anything_is_empty`.
- [ ] **Steps 2–4:** run (fail), implement, run (pass); gate. **Docs:** design §5.4 (the list order). Commit `feat: the suggestion list — top N in the ghost text's order`.

### Task 2: The wire and the timer (core)

**Interfaces — Produces:** `Request::ListConfig { size: u8, delay: Duration, empty: bool }` (`listcfg␟size␟delay_ms␟0|1`); `Response::List { id: u64, items: Vec<String> }` (`list␟id␟item…`); `Session::next_deadline(&self) -> Option<Instant>`; `Session::tick(&mut self, now: Instant) -> Option<Response>`; `serve_with` reads lines on a thread and waits with `recv_timeout` until the next deadline.

Session rules: a `suggest` (empty buffer allowed only when `empty`) arms one pending list `{ id, buffer, cwd, cdpath, due: now + delay }`, replacing any earlier one; `tick` at or past `due` builds the list (Task 1) and returns `List` if it has items; `ran`, `bye` and a config with size 0 disarm it.

- [ ] **Step 1: Failing tests:** protocol `listcfg_round_trips`, `list_round_trips_with_multiline_items`, `empty_buffer_suggest_decodes`; session `a_list_is_due_after_the_delay` (config 5/100 ms; `suggest` at t; `tick(t+50ms)` → None; `tick(t+100ms)` → `List` with the right id); `a_newer_suggest_rearms_the_list` (only the newer id is ever listed); `no_list_without_config_or_with_size_0`; `empty_line_list_only_when_enabled`; serve over pipes `serve_pushes_a_list_after_the_pause` (config delay 50 ms, one `suggest`, read: the `suggestion` at once, then `list` within 50–1000 ms) and `serve_sends_no_list_when_a_newer_request_came`.
- [ ] **Steps 2–4:** run (fail), implement, run (pass); gate. **Docs:** design §5.5 (the two messages, the empty `suggest`, the timer). Commit `feat(cli): the core times the pause and pushes the list`.

### Task 3: The zsh plugin draws the list

**Interfaces — Produces:** globals `_FUXX_AS_LIST_SIZE` (0 = off), `_FUXX_AS_LIST_DELAY_MS`, `_FUXX_AS_LIST_EMPTY`, `_FUXX_AS_LIST_SRC` (assoc: setting → `fuxx`/`shell`/`default`), `_FUXX_AS_LIST_ITEMS` (array), `_FUXX_AS_LIST_SEL` (0 = none), `_FUXX_AS_KEYSEQ`; widgets `fuxx-as-list-open`, `fuxx-as-list-close`; facts lines `setting=<name> <value> <source>`.

- [ ] **Step 1: Failing tests** (pty; `FUXX_AUTOSUGGESTIONS_LIST_DELAY=0.2` in `rc_before`): `list_appears_after_a_pause` (history of 3 `git` commands, type `git ` → after ~0.5 s the POSTDISPLAY has the ghost text then rows `  1  git …` …); `no_list_while_typing` (keys every 0.1 s for 1 s → never a row); `list_on_an_empty_line` (fresh prompt → rows); `list_off_and_empty_off_settings`; `a_late_list_is_not_drawn` (type, the list is due, then ← before it lands → no rows, and a list comes 0.2 s after the ←); `typing_closes_the_list`; `narrow_terminal_rows_are_cut` (`COLUMNS=20` in rc: rows ≤ 19 columns, ending with `…`); `other_plugins_highlights_survive` (a fake highlighter's `region_highlight` entry stays after open and close); `no_list_under_a_completion_list`; `fuxx_plugin_setting_wins_over_the_shell_variable` (`FUXX_PLUGIN_SETTING_LIST_SIZE=2` exported in `rc_before`, unset after; `FUXX_AUTOSUGGESTIONS_LIST_SIZE=5` → 2 rows; facts say `setting=list_size 2 fuxx`).
- [ ] **Steps 2–4:** run (fail), implement (settings at load/start, `listcfg` after `hello`, empty-line requests at line-init and when the buffer empties, key counter and re-requests in the wrappers, the `list` handler, drawing with a highlight array), run (pass); matrix; gate. **Docs:** design §6 (drawing), §7 (the variables and precedence). Commit `feat(zsh): the suggestion list under the line`.

### Task 4: The list keys

**Interfaces — Produces:** keymap `fuxx-as-list`; widgets `fuxx-as-list-down`, `fuxx-as-list-up`, `fuxx-as-list-pick` (reads `$KEYS`), `fuxx-as-list-enter`, `fuxx-as-list-tab`, `fuxx-as-list-esc`.

- [ ] **Step 1: Failing tests** (pty): `down_highlights_and_the_ghost_follows` (↓ ↓ → item 2 highlighted, POSTDISPLAY's ghost part = item 2's rest); `up_with_nothing_highlighted_is_history` (↑ → the previous history line, list gone); `up_inside_the_list_moves_up`; `alt_digit_puts_the_item_in_the_line` (`^[2` → BUFFER = item 2, nothing ran, keymap back to `main`); `option_characters_pick_too` (`¡` → item 1); `option_characters_type_when_no_list` (`£` with no list → typed); `enter_with_a_highlight_inserts_without_running` (no new prompt; BUFFER = the item); `enter_without_a_highlight_runs_the_line`; `tab_with_a_highlight_inserts`, `tab_without_a_highlight_is_todays_tab`; `esc_closes_the_list_and_the_ghost`; `pick_beyond_the_list_does_nothing`; `vi_mode_list_and_esc_enters_vicmd`; `keymap_is_restored_after_every_close` (after each of: pick, Enter, Esc, typing, accept-line → `$KEYMAP` is the previous one).
- [ ] **Steps 2–4:** run (fail), implement, run (pass); matrix; gate. **Docs:** design §6 (the keys), README (the list, ⌥1–⌥5 and the US-layout note). Commit `feat(zsh): list keys — ↓/↑, ⌥1–⌥5, Enter and Tab insert, Esc closes`.

### Task 5: doctor, latency, docs

- [ ] **Failing tests:** `doctor.rs` `report_lists_the_list_settings` (facts `setting=list on fuxx`, `setting=list_delay 3 default` … → lines `list         on (fuxx Settings)`, `list delay   3 s (default)`, `list size    5 (default)`, `list empty   on (shell)`).
- [ ] `cli/tests/latency.rs`: time `suggestion_list` for 5 items on the 100,000-line history (release; p95 < 10 ms, empty buffer and 1–6 character prefixes).
- [ ] Implement, run, gate, matrix. **Docs:** design §3 (measured), §15 row, roadmap Progress log. Commit `feat(cli): doctor shows the list settings; list latency measured`.

### Task 6: Whole-branch check and the user's check

- [ ] Full gate, matrix, release benchmarks; one whole-branch review (most capable model); one fix pass; again.
- [ ] A test release when the user asks; their check in fuxx (and the fuxx Settings milestone is planned in fuxx's repo after its M11aq); then M1c ✅, merge, roadmap.

## Self-review (2026-11-22)

- Spec coverage: list content and order incl. the empty line (T1), the core's timer and wire (T2),
  drawing, settings and precedence, any-key restart, late lists dropped (T3), every key of §4.3
  and vi mode (T4), doctor and latency (T5). fuxx's Settings UI is fuxx's milestone (spec §6).
- Types: `suggestion_list` (T1) used by `Session::tick` (T2); `ListConfig`/`List` (T2) spoken by
  the adapter (T3); facts `setting=` (T3) read by doctor (T5).

### Task 7: The look (added 2026-11-22; spec §9)

**Interfaces:** `_fuxx_as_list_draw` only. Highlights: the ghost part `fg=8`, the title and hint
`fg=8`, each number `fg=blue`, the selected row `bg=8` (its number `fg=blue,bg=8`); commands get no
entry (normal brightness). Rows: `"  N  cmd"`, the selected one `"▸ N  cmd"`. Space: items =
min(N, max(1, LINES-3)); the title shows when one more row fits, the hint when two do.

- [ ] **Tests first** (pty): `list_has_a_title_a_hint_and_colours` (line 2 is `  Suggestions`, the
  last line `  ⌥1–5 pick · ↑↓ move · esc close`, an `fg=blue` entry; after ↓ a line starts `▸ 1`
  and a `bg=8` entry exists); `short_window_drops_the_hint_first` (40×7: title, 3 items, no hint).
  Existing tests: `rows()` keeps item rows only; `standout` checks become `bg=8`; the exact
  POSTDISPLAY in `list_appears_after_a_pause` gains the title and hint.
- [ ] Implement, matrix, gate, docs (design §6), commit; then test release v0.0.10 (the user:
  "deploy it directly").
