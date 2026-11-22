# fuxx-completes: safety and polish (M1d) — design

**Status:** written 2026-11-22; builder check done (five questions, all settled as AI's calls
below); for the user's review. Not planned, not built. "Design §n" means
`docs/design.md`.

## 1. Intent and why
Three analyst agents ranked what to build next (2026-11-22). The user picked five items, "let's go
do #3–#7": a secret filter, a dangerous-command guard, worktree ranking, fix after failure, and
`why` + `doctor --report`. Why now: the suggestion list (M1c) shows history on its own after a
pause, even on an empty line, so a token or a destructive command is one keypress (⌥1) from the
line; and agent-heavy work multiplies worktrees and failed commands.

**Success:** no secret-looking line is ever shown; a destructive command is never offered unasked;
a habit in a repo carries to its subfolders and worktrees; after a failed command the right one
is one keypress away; a user can see why something was suggested.

## 2. Goals, non-goals, anti-goals
- **Goals:** the five parts below, all local, all in the Rust core (rule 5: no smarts in shell
  code; the zsh plugin only forwards facts).
- **Non-goals:** masking secrets (a masked line is a wrong line: dropped instead); a confirm prompt
  for dangerous commands (shell-framework territory, design §2); running anything to check a fix;
  AI suggestions.
- **Anti-goals:** a fix or list item that runs anything; slowing an answer past 10 ms p95; a
  false-positive filter hiding a common command (e.g. `git push` without force, `rm file`).

## 3. Approaches
1. **Everything in the core — picked.** Filters, tiers and fixes are ranking rules next to M1/M1b's;
   the plugin forwards the exit status. One place, testable, same in every shell later.
2. Filters in the zsh plugin: breaks rule 5 and would have to be rewritten for bash and fish.
3. A separate "safety" pass over the plugin's output: duplicates the ranking walk; slower.

## 4. The five parts

### 4.1 Secret filter
A history line is never suggested (ghost text, list, fix) when it looks like it holds a secret:
- known token shapes: `ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_`/`github_pat_`, `glpat-`, `sk-`/`sk-ant-`,
  `xox[abps]-`, `AKIA`/`ASIA` + 16 capitals or digits, `AIza` + 35, `npm_` + 36, `-----BEGIN`;
- `Authorization:` or `Bearer ` followed by 16+ characters;
- a URL with a password (`scheme://user:pass@`);
- an assignment or flag whose name holds `TOKEN`, `SECRET`, `PASSWORD`, `PASSWD`, `API_KEY`,
  `APIKEY`, `PRIVATE_KEY` or `ACCESS_KEY` with a literal value (`=$VAR` and `=$(…)` are fine);
- one word of 32+ characters of base64/hex letters with high entropy (≥ 3.5 bits per character).
Dropped, not masked; always on (**AI's call:** no switch; a false positive costs one suggestion).
**Pure hex words are not secrets** (**AI's call**, builder check): git SHAs and `sha256:` digests
(7–64 hex characters) score high entropy but are everyday arguments (`git show <sha>`,
`docker pull …@sha256:…`); a hex value is still hidden when its name says secret (`TOKEN=ab12…`).
The context file is unaffected (it never held text).

### 4.2 Dangerous-command guard
A line is "dangerous" when its first simple command (after `sudo`/`doas`, `builtin`/`command`,
`NAME=value`) is one of: `rm` with `-r`/`-R`/`-f` (any combination), `git push` with
`--force`/`-f`/`--force-with-lease`, `git reset --hard`, `git clean` with `-f`, `git checkout -- .`,
`git restore .`, `kubectl delete`, `terraform apply`/`destroy`, `helm uninstall`,
`docker system prune`, `docker volume rm`, `dd`, `mkfs*`, `chmod -R`, `chown -R`, `shutdown`,
`reboot`. Such a line:
- gets no tier-1 lift and no frequency boost (it ranks by recency like a line with no records);
- never appears in the empty-line list, as a fix (§4.4), or as ghost text on an empty line;
- is shown (ghost text or list) only once the typed text reaches its dangerous part: the command
  word and the dangerous flag or subcommand (`rm -`, `git push -`, `git reset --h`,
  `kubectl delete`) (**AI's call**, builder check: typing `g` must never put `git push --force`
  one → away); from there it ranks as usual. Guarded, not banned.
The list lives in the core as data, like `cd`/`open` (**AI's call** on the exact entries).

### 4.3 Worktree and repo ranking
Each folder belongs to a **repo**: the nearest `.git` above it; a worktree's `.git` file points
into `<common>/worktrees/<name>`, so all worktrees of one repo share the key `<common>`. The order
becomes: **this folder's history** (cwd exactly, as M1b) → **this repo's history** (successful
records in other folders of the same repo, ranked like tier 1) → project commands → the rest.
Repo keys are found by walking up (≤ 8 levels, as project detection) and cached in memory per
folder (**AI's call:** 256 folders); a folder outside any repo has no repo tier. **Calls from the
builder check:** a folder that no longer exists (a removed worktree) has no repo key, so its
records stop counting for the repo (accepted; nothing new is stored to remember it); a line that
only failed in the cwd stays in M1's failed-only tier even if it succeeded elsewhere in the repo
(this folder wins, as in M1).

### 4.4 Fix after failure (the user's choice: A)
On the **empty line right after a command failed** (non-zero, not ⌃C's 130 or 143), the core
offers a fix as **ghost text on the empty line**, and as item 1 of the list when it appears.
→/Tab/End take it; typing replaces it as usual. Candidates, first found wins:
1. **Package manager swap:** the failed command starts with `npm`/`yarn`/`pnpm`/`bun` (or
   `npx`/`pnpx`/`bunx`) and this folder's project uses another manager (M1b's detection) → the
   same arguments with the right one (`npm run dev` → `pnpm run dev`, `npm i` → `pnpm install`).
2. **Typo in the command word:** edit distance ≤ 2 between the failed first word and the first
   word of this folder's project commands or successful history → the same arguments with that
   word (`cagro test` → `cargo test`).
3. **Closest successful line:** a successful line from this folder's or repo's history within
   edit distance ≤ 2 of the whole failed line (`git psuh` → `git push`).
Never a dangerous or secret-looking line, never the failed line itself. Nothing runs.
**When rules 2 and 3 fire** (**AI's call**, builder check): only when the failed line has never
succeeded anywhere (no successful record and not a project command): that is a typo, while a
`git pull` or `make test` that failed but worked before is a real failure and gets no "fix". Rule 1
(the package manager) fires on any failure.
**Wire:** the plugin's `prev` message gains the exit status as an optional third field
(`prev␟cmd␟status`); the plugin asks on the empty line after a failure even when the empty-line
list is off. An M1c core drops the three-field `prev` as malformed (harmless: no fix).

### 4.5 `why` and `doctor --report`
- `fuxx-completes why <command line>` (through the shell function, which passes `--cwd $PWD` and
  `--histfile $HISTFILE`): loads the same history, context and projects as `serve` and prints why
  that line ranks where it does: runs here/in this repo/anywhere (ok, failed, newest), whether it
  followed the previous command, the project and file it came from, and flags (secret-looking:
  never shown; dangerous: guarded; `cd` to a missing folder). Local only.
- `fuxx-completes doctor --report`: the doctor output plus OS version, `TERM_PROGRAM`/`TERM` and
  the zsh version, inside a ```text fence, with a first line asking the user to check it before
  posting (it shows the data folder's path). Never sends anything.

## 5. Constraints
10 ms p95 answers (design §3), measured with the new rules on the 100,000-line benchmark; no new
dependency; nothing new on disk; zsh 5.3.1/5.8.1/5.9.

## 6. Milestone
**M1d, safety and polish**, after M1c and before M2 (**AI's call**). Release as a test build when
done.

## 7. Left to the builder
Exact token regexes and the entropy cut-off within §4.1's list; the edit-distance function;
`why`'s layout; where the dangerous list lives (a const table or `projects`-style data).
