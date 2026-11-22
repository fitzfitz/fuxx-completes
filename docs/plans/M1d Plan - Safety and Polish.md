# M1d Safety and Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Written 2026-11-22 after the user approved the design. Built in the session (executing-plans),
then one whole-branch review on the most capable model and one fix pass (the user's standing
choice for milestones). Decisions marked **AI's call** are open to change; each lands in
`docs/design.md` in the task named.

**Goal:** No secret-looking line is ever suggested; a destructive command is never offered unasked;
a habit in a repo carries to its subfolders and worktrees; after a failed command a fix is one
keypress away on the empty line; `why` and `doctor --report` explain what the core does.

**Architecture:** Everything is a ranking rule in the Rust core (spec §3, approach 1). A new
`core/src/safety.rs` judges lines (secret-looking, dangerous and how much of it must be typed); a
new `core/src/repo.rs` finds a folder's repo and the context records of its other folders; the
ranking gets a repo tier; `fuxx-autosuggestions` gains `fix_after_failure`. The zsh plugin only
forwards the exit status (`prev␟cmd␟status`) and asks on the empty line after a failure; the
`fuxx-completes` shell function passes `--cwd`/`--histfile` to `why`.

**Tech Stack:** Rust 2021, std only (no new dependency; nothing to look up in Context7); zsh 5.3+.

**Spec:** `docs/superpowers/specs/2026-11-22-safety-and-polish-design.md`; `docs/design.md` §3,
§5.4, §5.5, §6, §7. Rules: `AGENTS.md`.

## Decisions this plan settles

| Question | **AI's call** | Why |
| --- | --- | --- |
| Where the rules live | `core/src/safety.rs` (secret and danger, the danger list as a const table), `core/src/repo.rs` (repo keys and cache), `fuxx-autosuggestions/src/fix.rs` (the fix) | One file per rule set; the table is data like `cd`/`open` (spec §7) |
| Secret shapes | Token prefixes at a word start: `gh[pousr]_`+30, `github_pat_`+20, `glpat-`+20, `sk-`+20, `xox[abps]-`+10, `AKIA`/`ASIA`+16 capitals/digits exactly, `AIza`+35, `npm_`+36, `-----BEGIN` anywhere | Spec §4.1; minimum tails keep `sk-` in `task-x` or `npm_config` out |
| `Authorization:`/`Bearer`, URL passwords | The value (the run after `Bearer `, or after `Authorization:` and an optional scheme word, up to a space or quote) counts at 16+ characters unless it starts with `$`; a URL password counts unless it starts with `$` | `curl -H "Authorization: Bearer $TOKEN"` is everyday and holds no secret |
| Secret names | A word `NAME=value` or `--name=value`, or `--name value`, whose name (upper-cased, `-` → `_`) holds `TOKEN SECRET PASSWORD PASSWD API_KEY APIKEY PRIVATE_KEY ACCESS_KEY` and doesn't end in `FILE`, `PATH` or `DIR`, with a literal value (not empty, not starting with `$` or a backtick after one quote layer) | Spec §4.1; `--password-file x` names a file, not a secret |
| The entropy rule | A run of 32+ characters from `[A-Za-z0-9+/=_-]`, not all hex digits (dashes aside), with a letter and a digit, at most one `-`/`_`/`/` per 16 characters, and Shannon entropy ≥ 3.5 bits per character | Spec §4.1 plus the anti-goal: snake_case test names, branch names, paths and UUIDs have many separators or no digits; random tokens have few |
| Dangerous list and "reach" | A const table; each entry is the command words, a matcher for the dangerous word, and how much of that word must be typed: the whole subcommand word (`kubectl delete`, `terraform apply`, `docker system prune`); a flag's first character (`rm -`, `git push -`, `git clean -`, `chmod -`), except `git reset --h` (3), `git checkout --` (the `--` word, then a `.` argument) and `git restore .`; `dd`, `mkfs*`, `shutdown`, `reboot` the command word itself | Spec §4.2's examples; typing `g` never reaches `git push --force` |
| Prefix words | `sudo`/`doas` (and their options; `-u -g -C -h -p -r -t -T -D -U` take a value), `builtin`/`command`, `NAME=value`, all before the command word; only the first simple command counts (`cd x && rm -rf b` isn't dangerous) | Spec §4.2 |
| Dangerous ranking | Never in tier 1, the repo tier, this folder's history step or the follows set, never deferred as failed-only: it walks by recency with the rest, at every buffer length | Spec §4.2's first bullet is unconditional; "ranks as usual" read as "is shown" |
| Hidden lines and the walk's bounds | Secret and unreached dangerous lines are filtered out of the candidate sequence before the walk, so they never count toward the 50-candidate bound and are never returned by the budget fallback | A hidden line must never come back unchecked |
| Repo key | Walk up ≤ 8 folders, never looking at `$HOME` or `/` (as project detection); the first `.git`: a folder → its canonical path; a file `gitdir: X` → X (relative to that folder), and when X's parent is named `worktrees` and `X/commondir` exists, the canonical common folder. Missing folder → none | Spec §4.3; a dotfiles `.git` at `$HOME` must not join every folder |
| Which folders are "the repo's" | The context index's folders under one of the repo's roots (the main worktree, parent of a `.git` common folder, and each `<common>/worktrees/*/gitdir`'s parent) whose own key is the same | A prefix test over the index is cheap; a nested repo is excluded by its key |
| Repo cache | Folder → key for 256 folders (least recently used dropped), an entry older than 60 s looked up again; the repo's merged tallies cached per (cwd, records count) in the session | Spec §4.3's 256; a new worktree shows up within a minute; per-keystroke cost stays a hash lookup |
| Repo tier order | Like tier 1 (follows, successes in the repo's other folders with no boost for `cd`/`pushd`/`open`, newest there, recency); excludes tier 1 and lines failed-only in the cwd | Spec §4.3 |
| Wire | `prev␟cmd␟status` (status optional; `prev␟` with no command stays two fields); the plugin sends the status with every known `prev` | Spec §4.4 |
| When the plugin asks on an empty line | As now (list on and empty-line on), or when the last entered line's status was non-zero (`_FUXX_AS_FAILED`); the core decides 130/143 and everything else | Rule 5: the shell forwards a fact |
| Fix scope | Holds for the whole prompt (until the next `prev`); typing hides it as any ghost text, emptying the line brings it back | One state in the core |
| Fix rules' details | Rule 1 learns the manager from the folder's project commands (the first one starting with `npm`/`pnpm`/`yarn`/`bun`); `i`/`install` with packages → `add` (yarn, pnpm, bun) and `add` → `install` (npm); `i` alone → `install`; runners `npx`/`pnpx`/`bunx` → `npx`, `pnpm dlx`, `yarn dlx`, `bunx`. Rule 2's words: project commands, then this folder's, the repo's and every successful line (the context index); the failed word must not be one of them; distance is optimal string alignment (a swap is 1), ≤ 2 and below the word's length. Rule 3: this folder's then the repo's successful lines, smallest distance first. Rules 2–3 need a context index | `gti` → `git` is one swap; `cd nope` never becomes `ls nope` |
| `why` | `fuxx-completes why [--cwd DIR] [--histfile FILE] <line…>` (words joined by spaces); loads like `serve` (waits ≤ 5 s for the history), writes nothing (no compaction, no secret made); prints the step it ranks in, runs here / in this repo / anywhere (ok, failed, newest as an age), the project and file, and flags; the follows pairs live only in each shell's core, so it says so | Spec §4.5 |
| `doctor --report` | First line asks to check before posting; then a ```text fence around the doctor output plus `os` (macOS `SystemVersion.plist` ProductVersion, Linux `/etc/os-release` PRETTY_NAME, else `std::env::consts::OS`), `arch`, `terminal` (`TERM_PROGRAM` `TERM_PROGRAM_VERSION`, `TERM`); zsh's version is already the `shell` line. Nothing runs | Spec §4.5; reading two files beats running `sw_vers` |

## Global Constraints

- Suggestion answers stay under **10 ms at p95** on the 100,000-line benchmark (design §3), with
  every new rule on; measured at the end and written into design §3.
- **No new dependency; nothing new on disk** (no new file, no secret made by `why`); nothing
  leaves the machine.
- A fix or list item **never runs anything**; a dangerous line never shows on an empty line, in
  the empty-line list or as a fix; a secret-looking line never shows anywhere.
- zsh 5.3.1, 5.8.1 and 5.9 (`scripts/test-zsh-matrix.sh`); every adapter function starts with
  `emulate -L zsh`; nothing prints at startup or at the prompt; no smarts in shell code.
- Tests use a temp HOME and generated history and context files, never the user's.
- Per-task gate: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --check`; adapter tasks also the matrix. Docs (design + roadmap Progress log) with
  every task. Branch `m1d-safety-polish`; no merge, push, tag or release.

## Review Focus

1. **A common command hidden as a "secret"**: snake_case test names, branch names
   (`feature/JIRA-1234-…`), UUIDs, git SHAs and `sha256:` digests, long paths,
   `curl -H "Authorization: Bearer $TOKEN"`, `--password-file x`, `TOKEN=$(…)`. Test
   `everyday_lines_are_not_secrets` in Task 1.
2. **Prefixes and odd forms around a dangerous command**: `sudo -u root rm -rf x`, `FOO=1 rm -fr`,
   `command rm -R`, `rm -- -rf` (a file), `\rm -rf`, `git -C dir push -f`; and lines where the
   danger isn't in the first simple command. Test `prefixes_and_forms` in Task 2.
3. **Repos on odd layouts**: a bare repo, a submodule's `.git` file, a removed worktree, a `.git`
   at `$HOME`, an unreadable `.git` file: no repo tier, no error, no panic. Test
   `odd_layouts_have_no_or_their_own_key` in Task 3.
4. **A fix on the wrong prompt or one that runs**: after ⌃C's 130, after a success, after a line
   the filters dropped (leading space), after an empty Enter, and Enter on the empty line with a
   fix showing must run nothing. Tests `no_fix_after_success_or_interrupt` (session) and
   `enter_on_a_fix_runs_nothing` (pty) in Task 4.
5. **`why` and `doctor --report` on a bare setup**: no data folder, no `HISTFILE`, a line not in
   history, and nothing written to the data folder afterwards. Test
   `why_writes_nothing_and_works_without_data` in Task 5.

---

## File map

| File | Change |
| --- | --- |
| `core/src/safety.rs` (new) | `looks_secret`, `danger_reach`, `is_dangerous`, `hidden_for` |
| `core/src/cd_target.rs` | `command_words` (first simple command's words with byte ends) |
| `core/src/repo.rs` (new) | `repo_key`, `RepoCache`, `repo_tallies` |
| `core/src/context/index.rs` | `folders()`, `records()` |
| `core/src/rank.rs` | `Ranking.repo`; `repo_order`; dangerous lines get no lift |
| `core/src/protocol.rs` | `Request::Prev { command, status }` |
| `fuxx-autosuggestions/src/lib.rs`, `src/fix.rs` (new) | hidden lines filtered; repo step; `fix_after_failure` |
| `core/src/projects/detect.rs` | `Detection.origins` (for `why`) |
| `cli/src/session.rs`, `cli/src/why.rs` (new), `cli/src/main.rs`, `cli/src/doctor.rs` | repo tallies, fix, `why`, `--report` |
| `adapters/zsh/fuxx-autosuggestions.plugin.zsh` | status on `prev`, ask after a failure, `why` arguments |
| `cli/tests/{zsh_adapter,cli,serve,latency}.rs` | tests |
| `docs/design.md`, `docs/roadmap.md`, `README.md` | per task |

---

### Task 1: The secret filter

**Interfaces — Produces:** `pub fn safety::looks_secret(line: &str) -> bool`; in
`fuxx-autosuggestions`, the candidate sequence of `best_entry` and `suggestion_list` drops lines
where it is true (before the walk).

- [ ] **Step 1: Failing tests** (`core/src/safety.rs`): `token_shapes_are_secrets` (`gh auth login --with-token ghp_` + 36 alnum; `github_pat_11ABC…`; `glpat-` + 20; `export OPENAI_API_KEY=sk-proj-…`; `sk-ant-api03-…`; `xoxb-123-456-abc`; `AKIAIOSFODNN7EXAMPLE`; `AIza` + 35; `npm_` + 36; `echo '-----BEGIN OPENSSH PRIVATE KEY-----'`); `headers_and_urls` (`curl -H 'Authorization: Bearer abcdef0123456789xyz'` and `Authorization: token …` → true; `Bearer $TOKEN` → false; `git clone https://me:hunter2pass@host/r` → true; `https://me:$PASS@host` and `https://host/a:b@c` → false); `named_values` (`TOKEN=abc`, `--password hunter2`, `--api-key=xyz`, `DB_PASSWD='x'`, `GITHUB_TOKEN=0123abcd` (hex under a secret name) → true; `TOKEN=$X`, `TOKEN=$(pass x)`, `TOKEN=`, `--password-file p`, `--token-path p` → false); `high_entropy_words` (`wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY` and a 40-char base62 token → true); `everyday_lines_are_not_secrets` (Review Focus 1: `cargo test test_suggestion_list_follows_the_three_steps`, `git checkout feature/JIRA-1234-add-the-thing-for-users`, `docker rm 550e8400-e29b-41d4-a716-446655440000`, `git show 3f2a9c1d8e7b6a5f4e3d2c1b0a9f8e7d6c5b4a39`, `docker pull x@sha256:` + 64 hex, `cd ~/Projects/some-really-long-folder-name/and/more`, `git push`, `rm file`, `task-runner sk-x` → false).
- [ ] **Step 2:** failing tests in `fuxx-autosuggestions`: `secret_lines_are_never_suggested_or_listed` (history newest `export TOKEN=abc123` over `export PATH=x`: `suggestion(…, "export ")` → `"PATH=x"`; the list for `export ` and the empty-line list hold no `TOKEN` line); `fifty_secret_lines_do_not_use_up_the_walk` (51 newer distinct secret lines over one plain match → the plain one).
- [ ] **Steps 3–4:** run (fail); implement `looks_secret` (hand-written scanners, `f64::log2`) and the filter; run (pass); gate. **Docs:** design §5.4 "Safety (M1d)" paragraph (the secret rule and its calls), §15 gets the M1d row; roadmap Progress log. Commit `feat(core): never suggest a secret-looking line`.

### Task 2: The dangerous-command guard

**Interfaces — Consumes:** `looks_secret` (Task 1). **Produces:** `pub fn cd_target::command_words(command: &str) -> Vec<CmdWord>` with `pub struct CmdWord { pub value: String, pub plain: bool, pub assign: bool, pub end: usize }` (`end` = byte offset after the word in `command`); `pub fn safety::danger_reach(line: &str) -> Option<usize>` (byte length of `line` that must be typed before it may show; `None` = not dangerous); `pub fn safety::is_dangerous(line) -> bool`; `pub fn safety::hidden_for(line: &str, buffer: &str) -> bool` (secret, or dangerous with `buffer.len() < reach`).

- [ ] **Step 1: Failing tests** (`safety.rs`): `the_list_is_dangerous` (one line per entry of spec §4.2, e.g. `rm -rf build`, `rm -f x`, `rm --recursive x`, `git push --force-with-lease`, `git push origin main -f`, `git reset --hard HEAD~1`, `git clean -fdx`, `git checkout -- .`, `git restore .`, `kubectl delete pod x`, `terraform destroy`, `helm uninstall r`, `docker system prune -a`, `docker volume rm v`, `dd if=a of=b`, `mkfs.ext4 /dev/x`, `chmod -R 777 .`, `chown -R me .`, `shutdown -h now`, `reboot`); `safe_neighbours` (`rm file`, `git push`, `git push -u origin x`, `git reset --soft HEAD~1`, `git clean -n`, `git checkout -b x`, `git restore src/a.rs`, `kubectl get pods`, `terraform plan`, `chmod +x a`, `ddgr x`, `cd x && rm -rf b` → not dangerous); `reach_is_the_dangerous_part` (`rm -rf build` → `"rm -".len()`, `git push --force` → `"git push -".len()`, `git reset --hard` → `"git reset --h".len()`, `kubectl delete pod x` → `"kubectl delete".len()`, `reboot` → 6); `prefixes_and_forms` (Review Focus 2: `sudo -u root rm -rf x` and `FOO=1 command rm -fr x` dangerous with reach counted in the whole line; `rm -- -rf` not; `\rm -rf x` (quoted word) dangerous; `git -C dir push -f` dangerous).
- [ ] **Step 2:** failing tests in `core/src/rank.rs`: `dangerous_lines_get_no_lift` (`rm -rf target` 9 successes here and `rm -rf dist` newest with none → order `rm -rf dist`, `rm -rf target`; not in `here_order`; not first by follows); in `fuxx-autosuggestions`: `dangerous_needs_its_dangerous_part_typed` (history `git push --force` newest over `git pull`: `g` → `it pull`, `git push -` → `-force`, `git push --force` exact → none); `no_dangerous_line_on_the_empty_line_list` (successors `git add .` → `git push -f`, previous `git add .` → empty-line list lacks it).
- [ ] **Steps 3–4:** run (fail); implement (`CmdWord` by tracking byte offsets in `first_simple_command`; the table; filters in `rank.rs` lifts, `failed_only` skip, and the two candidate sequences); run (pass); gate. **Docs:** design §5.4 (the guard, the table, reach); roadmap. Commit `feat(core): guard dangerous commands until their dangerous part is typed`.

### Task 3: Repo and worktree ranking

**Interfaces — Produces:** `pub fn repo::repo_key(folder: &Path, home: Option<&Path>) -> Option<PathBuf>`; `pub struct repo::RepoCache` with `new()`, `key(&mut self, folder, home) -> Option<PathBuf>`, `tallies(&mut self, index: &ContextIndex, cwd: &Path, home) -> Option<HashMap<Key, Tally>>` (merged tallies of the repo's other folders; `None` outside a repo); `ContextIndex::folders(&self) -> impl Iterator<Item = (&Path, &HashMap<Key, Tally>)>` and `records(&self) -> u64`; `Ranking.repo: Option<&'r HashMap<Key, Tally>>`; `pub fn rank::repo_order<'a>(history: &'a History, prefix: &str, ranking: &Ranking) -> Vec<&'a str>`; `History::ranked_repo` wrapper. Order everywhere: here → repo → project → rest; the empty-line list: follows → here → repo → project → newest.

- [ ] **Step 1: Failing tests** (`repo.rs`, temp folders with hand-made `.git` layouts, no git binary): `subfolders_share_the_repo_key`; `worktrees_share_the_common_key` (`main/.git/` + `main/.git/worktrees/wt/{gitdir,commondir}` + `wt/.git` file); `odd_layouts_have_no_or_their_own_key` (Review Focus 3: missing folder → None; `.git` at the given home → None; submodule `.git` file pointing to `super/.git/modules/m` → that folder; unreadable/garbage `.git` file → None, no panic; bare `x.git` with a worktree → the bare folder); `tallies_merge_other_folders_of_the_repo` (records in `main/a`, `wt/b`, an unrelated folder and the cwd → only `a` and `b` summed); `cache_is_bounded` (257 folders → 256 entries).
- [ ] **Step 2:** failing tests in `rank.rs`: `repo_tier_sits_between_here_and_the_rest` (success in another repo folder beats a newer line with no records; loses to a success here); `failed_here_stays_failed_only` (succeeded in the repo, failed only in the cwd → last); in `fuxx-autosuggestions`: `repo_history_beats_project_commands`; in `session.rs`: `a_worktree_learns_from_the_main_checkout` (real temp folders, `ran` in `main/sub`, suggest in `wt` → that line).
- [ ] **Steps 3–4:** run (fail); implement; run (pass); gate. **Docs:** design §5.4 (repo tier, keys, cache, the calls); roadmap. Commit `feat(core): a repo's history ranks after this folder's, across worktrees`.

### Task 4: Fix after failure

**Interfaces — Consumes:** `hidden_for`, `is_dangerous`, `looks_secret` (Tasks 1–2), `Ranking.repo` (Task 3). **Produces:** `Request::Prev { command: Option<String>, status: Option<i32> }`; `pub fn fuxx_autosuggestions::fix_after_failure(history: &History, failed: &str, ranking: &Ranking, projects: &[String]) -> Option<String>` (the whole fixed line); `pub fn fix::edit_distance(a: &str, b: &str) -> usize` (optimal string alignment over chars); the session answers an empty-buffer `suggest` with the fix when the previous status failed (non-zero, not 130/143), and puts it first in the empty-line list; plugin global `_FUXX_AS_FAILED`.

- [ ] **Step 1: Failing tests** (protocol): `prev_with_status_round_trips` (`prev␟make␟2`), `prev_without_status_still_decodes`, `prev_with_a_bad_status_is_malformed`. (fix.rs): `package_manager_swap` (projects `["pnpm install","pnpm run dev"]`: `npm run dev` → `pnpm run dev`, `npm i` → `pnpm install`, `npm install react` → `pnpm add react`, `npx vite` → `pnpm dlx vite`; in an npm folder → None); `typo_in_the_command_word` (successful `cargo test` anywhere: `cagro test` → `cargo test`; `gti status` → `git status`); `closest_successful_line` (`git psuh` with `git push` ok here → `git push`); `never_for_a_real_failure` (`make test` that once succeeded → None; `cd nope` with `ls` known → None); `never_dangerous_secret_or_itself`; `edit_distance_counts_a_swap_as_one`. (session): `no_fix_after_success_or_interrupt` (Review Focus 4: status 0, 130, 143, unknown prev → empty buffer gets None); `fix_is_ghost_text_and_list_item_one`.
- [ ] **Step 2: Failing tests** (pty, list off as the harness default): `fix_shows_on_the_empty_line_after_a_failure` (`ls` then `lss` → empty line ghost `ls`, → accepts it, nothing runs); `package_manager_fix` (folder with `package.json` + `pnpm-lock.yaml`; `npm run dev` fails (no npm on PATH) → ghost `pnpm run dev`); `no_fix_after_ctrl_c_status` (`sh -c 'exit 130'` → no ghost); `enter_on_a_fix_runs_nothing` (Review Focus 4: Enter → a new prompt, the history gains nothing); `prev_carries_the_status` (stub core recorder: `prev␟false␟1`).
- [ ] **Steps 3–4:** run (fail); implement; run (pass); matrix; gate. **Docs:** design §5.4 (fix rules and calls), §5.5 (`prev`'s status field), §6 (asking after a failure); README (the fix); roadmap. Commit `feat: a fix after a failed command, as ghost text on the empty line`.

### Task 5: `why` and `doctor --report`

**Interfaces — Consumes:** everything above. **Produces:** `Detection.origins: Vec<Origin>` (one per candidate: `pub struct Origin { pub id: &'static str, pub folder: usize /* index into Detection.folders */, pub file: Option<&'static str> }`, plus `Detection.folders: Vec<PathBuf>`); `pub fn why::explain(…) -> String` called by `Session::explain(&mut self, line: &str, cwd: &Path) -> String`; `Session::for_why(data, home)` (no compaction); `doctor::report(…, report: bool)`.

- [ ] **Step 1: Failing tests** (`cli/tests/cli.rs`, real processes, temp HOME): `why_explains_a_line_that_ran_here` (records via `append` + `own_file_id`: lines `folder`, `here       ok 2 · failed 0`, `step       this folder's history`); `why_flags_secret_and_dangerous_lines` (`never shown: looks like it holds a secret`, `guarded: shown once you type "rm -"`); `why_names_the_project_and_file` (pnpm folder: `project    pnpm (package.json)`); `why_writes_nothing_and_works_without_data` (Review Focus 5: no HOME data, no histfile, a line not in history → `not in your history`; the data folder's listing is unchanged afterwards); `doctor_report_is_fenced_with_a_warning` (first line `Check this before you post it: it shows your data folder's path.`, then ```` ```text ````, `os `, `arch `, `terminal `, last line ```` ``` ````). (pty): `the_shell_function_passes_cwd_and_histfile_to_why`.
- [ ] **Steps 2–4:** run (fail); implement; run (pass); matrix; gate. **Docs:** design §7 (`why`, `--report`); README (both commands); roadmap. Commit `feat(cli): why and doctor --report`.

### Task 6: Measure, review, fix

- [ ] `cargo test --release -p fuxx-completes --test latency -- --ignored --nocapture` and `--test size -- --ignored --nocapture`; add a repo case to `latency.rs` (the 200 folders as real folders of one repo: worst case for the repo tier); record numbers in design §3. If the binary passes `size.rs`'s budget, stop and say so (a design change).
- [ ] One whole-branch review (superpowers:requesting-code-review, most capable model, this Review Focus); one fix pass, each fix test-first; full gate and matrix again; roadmap row "built and reviewed, waiting for the user's check".

## Self-review (2026-11-22)

- Spec coverage: §4.1 (T1), §4.2 incl. the walk and empty line (T2, T4 for the fix), §4.3 incl.
  the removed worktree and failed-here rule (T3), §4.4 rules 1–3, wire, ask-when-off (T4), §4.5
  (T5), §5 latency, no dependency, matrix (T6, gate).
- Types: `hidden_for` (T2) used by T4's fix filter; `Ranking.repo` (T3) used by T4's rule 3 and
  T5's `why`; `Prev.status` (T4) read by the session only.
