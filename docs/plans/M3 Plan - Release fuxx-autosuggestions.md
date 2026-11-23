# M3 Release fuxx-autosuggestions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Written 2026-11-23 from `docs/design.md` §15's M3 row (designed and approved 2026-11-09, amended
2026-11-13). Built in the session (executing-plans): Tasks 1–5 on branch `m3-release`, then one
whole-branch review on the most capable model, one fix pass, then Task 6, the release, **one
approval gate at a time**. Decisions marked **AI's call** are open to change; each lands in
`docs/design.md` in the task named. **Six decisions are the user's** (below). **Answered
2026-11-23:** the user, "go with your picks": R1 `0.1.0`, R2 C1, R3 T1, R4 X1, R5 S1, R6 A1 (the
table's AI's picks, now the user's decisions; recorded in `docs/design.md` §4).

**Goal:** fuxx-autosuggestions gets its first real release: a Homebrew tap, zsh plugin manager
lines, the three crates on crates.io, a README install section that works, and
`fuxx-plugin-macos-arm64.zip` plus `fuxx-plugin-macos-x86_64.zip` on the release, installed and
checked in fuxx.

**Architecture:** No new product behaviour. The crates become publishable (the project mappings
move inside the core crate, metadata, licence files). One script builds both chips' binaries on
this Mac and makes the two zips with every check the hand recipe did, and more; a second renders
the Homebrew formula from a release's real checksums. The formula installs the prebuilt binary
from the same release zips and the three adapters from the tagged source. Everything outward
(push, tag, release, tap, crates.io) happens only in Task 6, each step behind the user's word.

**Tech Stack:** Rust 2021 (cargo 1.97.1), targets `aarch64-apple-darwin` and `x86_64-apple-darwin`
(both installed), `ditto`, `lipo`, `codesign`, Rosetta 2, Homebrew 7.0.9 (formula Ruby DSL), `gh`.

**Spec:** `docs/design.md` §2, §3 (platforms), §4, §5.6, §7, §8, §12, §13, §14, §15 (M3 row, "Left to
the builder": packaging details), §16; `THIRD_PARTY_NOTICES.md`, `LICENSE-MIT`, `LICENSE-APACHE`.
Rules: `AGENTS.md`.

## What was checked, and where (2026-11-23)

| Fact | Source |
| --- | --- |
| `fuxx-completes`, `fuxx-completes-core`, `fuxx-autosuggestions`, `fuxx-autocomplete` (and `fuxx`) are **free** on crates.io (`GET /api/v1/crates/<name>` → 404, which also covers the `_` spellings) | crates.io API, read-only |
| **Blocker:** `cargo package -p fuxx-completes-core` fails verification: `build.rs` reads `$CARGO_MANIFEST_DIR/../projects`, outside the package, so the tarball has no mappings (`projects/: No such file or directory`) | local `cargo package` run (output deleted) |
| No licence file is in any crate package (`cargo package --list`): the licences sit only at the repo root | same run |
| Path dependencies need a `version` to publish; with both, the path is used locally and the registry version when published | Context7 `/websites/doc_rust-lang_cargo` (specifying-dependencies) |
| `cargo publish --workspace` publishes members in dependency order and waits for each to show in the index; `--exclude`, `--dry-run` exist in cargo 1.97.1 | Context7 `/rust-lang/cargo` (`cargo_publish.rs`, man page); `cargo publish --help` |
| crates.io needs `description` and `license`; ≤ 5 keywords, ≤ 5 categories from its slug list; `readme` is uploaded and rendered | Context7 `/websites/doc_rust-lang_cargo` (manifest) |
| crates.io policy: a crate that "exists only to reserve a name … without having any genuine functionality" may be removed; owners can **delete** a crate within 72 h, or later if it has one owner, < 1000 downloads a month and no dependents on crates.io; otherwise only **yank** | crates.io `svelte/src/routes/policies/+page.svelte` (rust-lang/crates.io, via `gh api`) |
| Tap repos are named `homebrew-<name>`, used as `brew install <user>/<name>/<formula>`; formulae go in `Formula/`; `brew tap-new` makes one locally | Context7 `/websites/brew_sh` (How-to-Create-and-Maintain-a-Tap) |
| Rust formulae: `depends_on "rust" => :build`, `cargo install *std_cargo_args(path: …)`; bottles for a tap come from its GitHub Actions and `brew pr-pull`; checks: `brew install --build-from-source`, `brew test`, `brew audit --strict --new --online`, `brew style` | Context7 `/websites/brew_sh` (Language-Specific-Formulae, Adding-Software, Formula-Cookbook) |
| Homebrew: Intel macOS is **Tier 3** since September 2026, no new Intel bottles, Intel support removed in or after September 2027; a prefix longer than the default is also Tier 3 | Context7 `/websites/brew_sh` (Support-Tiers) |
| This Mac's Homebrew lives at `~/.local/Homebrew` (non-default, Tier 3); the `rust` bottle is `cellar :any`, so a `depends_on "rust"` formula may have to build Rust from source here | `brew config`, `brew info --json=v2 rust` |
| `fuxx-completes`, `fuxx` are not Homebrew formulae or casks; `fitzfitz/homebrew-fuxx` and `fitzfitz/homebrew-tap` don't exist | formulae.brew.sh API; `gh api` |
| x86_64 on this arm64 Mac: `cargo build --release -p fuxx-completes --target x86_64-apple-darwin` builds in ~7 s; the binary is **783,184 bytes** (arm64: 718,752), only 3,248 under `cli/tests/size.rs`'s 786,432 limit; unsigned (arm64 is ad-hoc linker-signed), `minos 11.0`; it runs under Rosetta (`arch -x86_64 … nope` → the usage line, exit 2) | local build (deleted after) |
| fuxx picks `fuxx-plugin-macos-<chip>.zip` (`arm64`/`x86_64` by its own `ARCH`), else `fuxx-plugin-macos.zip`, else the source tarball, from `releases/latest`; it needs a program for this Mac in `bin/`, so **an Intel Mac can't install fuxx-completes today** ("This plugin has no program for this Mac."); extra files in the zip are ignored | fuxx `src/plugin_source.rs` `from_release`, `src/plugin_manifest.rs` `check_files`, `src/plugin_fetch.rs` |
| The v0.0.13 zip holds `fuxx-plugin.conf`, `bin/fuxx-completes`, `shell/fuxx-autosuggestions.plugin.zsh` at its root | `zipinfo` of the release asset (deleted after) |
| oh-my-zsh loads `$ZSH_CUSTOM/plugins/<name>/<name>.plugin.zsh`; antidote takes `path:<subfolder>`; zinit takes `pick"<file>"`; zap sources only root-level files of a cloned repo, or a local file path | Context7 `/ohmyzsh/wiki`, `/mattmc3/antidote`, `/zdharma-continuum/zinit`; zap's `zap.zsh` (master) |
| The zsh adapter finds itself with `${${(%):-%x}:A}` (works when sourced from inside a function, as plugin managers do); sourced non-interactively it prints nothing and returns | `adapters/zsh/fuxx-autosuggestions.plugin.zsh` lines 29–31, 119; `zsh -fc 'source …'` |
| `fuxx-completes doctor` with a fresh `HOME` prints `fuxx-completes <version>` first, writes nothing, exits 0 | local run |
| The binary links blake3 (CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception), arrayvec and cfg-if (MIT OR Apache-2.0), constant_time_eq (CC0-1.0 OR MIT-0 OR Apache-2.0); the rest are build-time only | `cargo tree -e normal`, `cargo metadata` |

## Decisions only the user can make

Most important first. Nothing in Task 6 happens before the matching answer.

**The user's answers (2026-11-23, "go with your picks"):** R1 **`0.1.0`**; R2 **C1** (three crates,
published from the user's own crates.io account; `fuxx-autocomplete` gets `publish = false`); R3
**T1** (`fitzfitz/homebrew-fuxx`, `brew install fitzfitz/fuxx/fuxx-completes`); R4 **X1** (x86_64
cross-built on this Mac; **the user's check:** one look on a real Intel Mac after the release, if
the user has one, Gate B); R5 **S1** (all three adapters in the formula and the README); R6 **A1**
(`init` and the demo GIF are not in M3; a design change to brainstorm after M3).

| # | Question | Options and cost | **AI's pick** |
| --- | --- | --- | --- |
| R1 | The first real version, and what it promises | **V1 `0.1.0`**: "usable, still pre-1.0": the crate APIs, the wire format and shell variables may change in a minor bump (Cargo's 0.x rule), the context file can be wiped by `forget`. **V2 `1.0.0`**: promises stable crate APIs and wire format, which M4–M8 (specs, line parsing, the menu, `state`) will break. **V3** stay on `0.0.x` (no "first release" signal; crates at 0.0.x match only the exact version) | **V1 `0.1.0`** |
| R2 | Which crates go to crates.io, under whose account | **C1** `fuxx-completes-core`, `fuxx-autosuggestions` and `fuxx-completes` (the binary, `cargo install fuxx-completes`), published by the user's own crates.io account (it needs a verified email; the user runs `cargo login`; the builder never sees the token); `fuxx-autocomplete` waits for M8 (`publish = false`). **C2** all four now (the empty `fuxx-autocomplete` is the "reserve a name" case the crates.io policy lets them remove). **C3** only the binary: not possible, it depends on the two libraries, which must be published first. **C4** none in M3 (changes design §15's M3 row) | **C1** |
| R3 | The tap's name and owner | **T1** `fitzfitz/homebrew-fuxx` → `brew install fitzfitz/fuxx/fuxx-completes` (room for fuxx itself later). **T2** `fitzfitz/homebrew-tap` → `fitzfitz/tap/…` (generic). **T3** `fitzfitz/homebrew-fuxx-completes` → `fitzfitz/fuxx-completes/fuxx-completes`. Any of them is a new public repo; §4's repo policy names repos `fuxx-…`, but Homebrew requires `homebrew-…` | **T1** |
| R4 | The x86_64 build route | **X1** cross-build on this Mac (checked: builds, runs under Rosetta; the whole test suite can run under Rosetta with `--target x86_64-apple-darwin`); nobody checks it on a real Intel Mac unless the user has one. **X2** one universal binary in `fuxx-plugin-macos.zip` (fuxx already falls back to it; changes design §8; the binary doubles to ~1.5 MB in each install). **X3** a GitHub Actions Intel runner (CI is not in the design; GitHub drops Intel runners in 2027). **X4** drop x86_64 (changes §8; Intel fuxx users keep getting "no program for this Mac") | **X1**, and if the user has an Intel Mac, one look there after the release |
| R5 | bash and fish in M3's install story | **S1** yes: the formula installs all three adapters, the README gives each shell its line (M2 built them; a few lines of docs and formula). **S2** zsh only, as the M3 row's wording ("zsh plugin manager install"); bash and fish stay "source it from a clone" | **S1** |
| R6 | The analysts' items (2026-11-22): a one-line `fuxx-completes init`, a README demo GIF | Neither is in the design (§7 lists the binary's commands; §15's M3 row has no demo). **A1** not in M3; bring them to a brainstorm after M3, as a design change. **A2** add them now: the design changes first (§7 for `init`, which must also decide whether it may write rc files: AGENTS.md forbids touching them only in tests, but nothing yet allows it), then this plan grows a task | **A1** |

## Decisions this plan settles (AI's calls)

| Question | **AI's call** | Why | Lands in |
| --- | --- | --- | --- |
| Where the mappings live | `projects/` moves to `core/projects/`; `build.rs` reads `$CARGO_MANIFEST_DIR/projects` | The only way the published core can build (the blocker above); one folder, no copies | §5.4, §12 |
| Crate metadata | `[workspace.package]` gains `homepage` (the GitHub page), `keywords = ["shell", "zsh", "bash", "autosuggestions", "history"]`, `categories = ["command-line-utilities"]`; each published crate gets a short `README.md` (what it is, a link to the repo) and `readme = "README.md"` | crates.io renders it; the root README's relative links break there | §12 |
| Inter-crate versions | `[workspace.dependencies]` holds `fuxx-completes-core = { path = "core", version = "=X" }` and `fuxx-autosuggestions = { path = "fuxx-autosuggestions", version = "=X" }`; members use `.workspace = true` | Exact pins: the three crates are built and released together, and 0.1.x of one must not pair with 0.1.y of another | §12 |
| Licence files in crates | `LICENSE-MIT` and `LICENSE-APACHE` copied (not linked) into `core/`, `fuxx-autosuggestions/`, `cli/`; a test keeps the copies byte-equal to the root's | MIT asks for the notice in copies; plain files package the same everywhere | §12 |
| Notices for the binary | `THIRD_PARTY_NOTICES.md` gains "Rust crates in the binary": blake3, arrayvec, cfg-if, constant_time_eq with versions and the licence each is used under (Apache-2.0 for all, whose text is `LICENSE-APACHE`) | Their licences ask for the notice to travel with copies of the binary | THIRD_PARTY_NOTICES.md |
| What the zips hold | The §8 three (`fuxx-plugin.conf`, `bin/fuxx-completes`, `shell/fuxx-autosuggestions.plugin.zsh`) plus `LICENSE-MIT`, `LICENSE-APACHE`, `THIRD_PARTY_NOTICES.md`, at the zip's root as today | Same reason; fuxx ignores extra files (checked) | §8 |
| Release tooling | `scripts/package.sh` (version from `Cargo.toml`): refuses a dirty tree and any version disagreement; `cargo build --release --locked -p fuxx-completes` for both targets; checks `lipo -archs`, size < 786,432 bytes each, `codesign -v` on arm64; stages and zips with `ditto -c -k --norsrc --noextattr`; unzips each into a temp folder and runs `bin/fuxx-completes doctor` (x86_64 through `arch -x86_64`) in a temp `HOME`, expecting `fuxx-completes <version>`; writes `target/dist/v<version>/` (the zips, `SHA256SUMS`); prints nothing else on success. `--allow-dirty` for dry runs only | The hand recipe, plus the checks it lacked; output stays in `target/` | §8, §15 |
| The formula's route | **Prebuilt**: `url` = the tag's source tarball (adapters, licences), and inside `on_arm`/`on_intel` a `resource "binary"` = that chip's release zip; `bin.install` the binary, `pkgshare.install` `adapters/zsh`, `adapters/bash`, `adapters/fish`; `license any_of: ["MIT", "Apache-2.0"]`; `depends_on :macos`; no bottles | Installs in seconds with no Rust toolchain (a `depends_on "rust"` build could compile Rust itself on a non-default prefix, as here); the same bytes fuxx installs; Homebrew builds no Intel bottles anyway. Fallback if `brew audit` refuses it: build from source with `std_cargo_args(path: "cli")` | §15 (packaging) |
| The formula's file | Rendered by `scripts/formula.sh <version> [--local]`: by default it downloads the release's zips and GitHub's tag tarball and hashes them; `--local` points `file://` URLs at `target/dist/` and a `git archive` tarball, for the pre-release check. The tap repo holds the only committed copy | No template folder in this repo; checksums always from the real bytes | §15 |
| The formula's `test do` | `assert_match "fuxx-completes #{version}", shell_output("#{bin}/fuxx-completes doctor")`; `shell_output("#{bin}/fuxx-completes nope 2>&1", 2)` matches `usage: fuxx-completes`; `shell_output("zsh -fc 'source #{pkgshare}/zsh/fuxx-autosuggestions.plugin.zsh'")` is empty | Runs offline, writes nothing; the adapter must print nothing (§3) | — |
| Caveats | The formula prints the three lines to add (zsh `source …`, bash `source …` at the end of `~/.bashrc`, fish `ln -s … ~/.config/fish/conf.d/`), with `HOMEBREW_PREFIX/share/fuxx-completes/…` paths that survive upgrades; it never edits rc files and never installs into fish's `vendor_conf.d` (that would turn reporting on for every fish user unasked) | The user opts in, per shell | §6, §13 |
| zsh plugin managers | No new file at the repo root. The README gives: plain `source` (Homebrew's path or a clone), oh-my-zsh (`ln -s <clone>/adapters/zsh $ZSH_CUSTOM/plugins/fuxx-autosuggestions` and `plugins+=(fuxx-autosuggestions)`), zinit (`zinit ice pick"adapters/zsh/fuxx-autosuggestions.plugin.zsh"; zinit light fitzfitz/fuxx-completes`), antidote (`fitzfitz/fuxx-completes path:adapters/zsh`), zap (clone, then `plug "<clone>/adapters/zsh/fuxx-autosuggestions.plugin.zsh"`). Plugin managers fetch only the shell file; the binary comes from Homebrew, `cargo install fuxx-completes` or `FUXX_COMPLETES_BIN` | §5.6/§12 keep one adapter file per library under `adapters/`; a root file would be picked wrongly by zap and zinit once `fuxx-autocomplete.plugin.zsh` exists (M8) | §5.6 |
| Where the release is made | Tag `v<version>` on `main` after the merge (not on the branch, as the test builds were) | The first real release comes from the reviewed, merged state | §15 |
| Order of the gates | Merge and push `main` → tag and GitHub release → the user's check in fuxx → tap → crates.io last | Each step only after the one it builds on is checked; the most permanent last | — |

## Global Constraints

- **No new product behaviour.** No new command, no new shell variable, no new dependency (tests
  parse `Cargo.toml` and `fuxx-plugin.conf` by lines). Release binary **< 786,432 bytes on each
  chip** (design §3; `cli/tests/size.rs`).
- **macOS only** (design §3: Linux after the first release of each library). Formula `depends_on
  :macos`; zips for `arm64` and `x86_64` only.
- The zip layout of design §8 at the zip's root, version in `fuxx-plugin.conf` equal to
  `Cargo.toml`'s; `format = 1`, `id = fuxx-completes`, `bin = bin`,
  `zsh = shell/fuxx-autosuggestions.plugin.zsh`.
- **Nothing outward without the user's word, one gate at a time** (Task 6): no push, merge, tag,
  release, repo creation, tap push or `cargo publish` (only `--dry-run`) by the builder alone.
- No build output outside `target/`; `scripts/package.sh` and `formula.sh` write only
  `target/dist/`; scratch folders (unzip checks, the local tap) are removed at the end of each
  step. Never run a bare `fuxx` (the real app is on `PATH`); every wait loop has a deadline.
- Tests never touch the user's rc files, history or context file (temp `HOME`, `ZDOTDIR`, `XDG_*`).
- Per-task gate: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --check`, plus `scripts/test-zsh-matrix.sh`,
  `scripts/test-bash-matrix.sh`, `scripts/test-fish-matrix.sh` when the task touches an adapter
  or its tests. Docs (design, roadmap Progress log, READMEs) with every task. Plain commit
  messages, no AI attribution lines.

## Review Focus

1. **The x86_64 binary is what it says and fits.** A zip named `x86_64` holding an arm64 binary,
   or an x86_64 binary over the size limit (3 KB of room today), would install and then fail on
   every Intel Mac. `scripts/package.sh` refuses both; Task 2's `x86_64_suite_passes_under_rosetta`
   step runs the whole test suite on the x86_64 build.
2. **Two installs in one shell, from different places**: Homebrew's copy sourced in `.zshrc` plus
   fuxx's plugin copy (from a folder with spaces, `~/Library/Application Support/…`). One copy
   runs, the other stays off and `doctor` names both paths; the data folder follows the copy that
   runs (§8). Test `brew_copy_and_fuxx_copy_one_runs` (Task 4).
3. **Plugin-manager load shapes**: sourced through a symlinked folder (oh-my-zsh), from inside a
   function (zinit, antidote), from a path with spaces. The adapter must find itself, load once,
   and keep `source ~/.zshrc` working. Test `loads_through_plugin_manager_shapes` (Task 4).
4. **Version skew**: `Cargo.toml`, `fuxx-plugin.conf`, the inter-crate pins, the tag and the
   formula disagreeing (fuxx would show one version and run another). Test `versions_agree`
   (Task 1); `scripts/package.sh` and `formula.sh` refuse a mismatch (Tasks 2, 3).
5. **A zip that unpacks wrong**: lost exec bit, AppleDouble `._` files, a nested folder (fuxx
   finds the root by `fuxx-plugin.conf`, Homebrew stages a nested folder differently). Task 2's
   unzip-and-run check and its `zipinfo` listing compare against the exact expected entries.

---

## File map

| File | Change |
| --- | --- |
| `projects/` → `core/projects/`; `core/build.rs`, `core/src/projects/mod.rs`, `core/Cargo.toml` | the mappings inside the crate |
| `Cargo.toml`, `core/Cargo.toml`, `fuxx-autosuggestions/Cargo.toml`, `cli/Cargo.toml`, `fuxx-autocomplete/Cargo.toml` | metadata, `[workspace.dependencies]`, `publish = false` |
| `core/`, `fuxx-autosuggestions/`, `cli/`: `README.md`, `LICENSE-MIT`, `LICENSE-APACHE` (new) | crate pages and licences |
| `cli/tests/release.rs` (new) | versions, plugin manifest, licence copies |
| `scripts/package.sh`, `scripts/formula.sh` (new) | the zips, the formula |
| `cli/tests/zsh_adapter.rs` | load shapes, two copies |
| `THIRD_PARTY_NOTICES.md`, `fuxx-plugin.conf` | crate notices; version |
| `README.md`, `adapters/{zsh,bash,fish}/README.md`, `docs/design.md`, `docs/roadmap.md` | per task |
| `fitzfitz/homebrew-fuxx` (new repo, Task 6 only): `Formula/fuxx-completes.rb`, `README.md` | the tap |

---

### Task 1: Crates that can be published

**Interfaces — Produces:** `[workspace.dependencies]` entries `fuxx-completes-core` and
`fuxx-autosuggestions` (path + `version = "=<workspace version>"`), used by `cli` and
`fuxx-autosuggestions`; `core/projects/*.toml` read by `core/build.rs`;
`cli/tests/release.rs` with a helper `fn field(text: &str, key: &str) -> Option<String>` (the
value of `key = value` / `key = "value"` on its own line, comments skipped). The scripts of Tasks
2 and 3 apply the same rule in shell: the version is `[workspace.package]`'s.

- [x] **Step 1: Failing tests** (`cli/tests/release.rs`): `versions_agree` (the
  `[workspace.package]` version, `fuxx-plugin.conf`'s `version`, both `[workspace.dependencies]`
  pins without their `=`, and `env!("CARGO_PKG_VERSION")` are all equal); `plugin_conf_matches_design`
  (`format` `1`, `id` `fuxx-completes`, `bin` `bin`, `zsh` `shell/fuxx-autosuggestions.plugin.zsh`,
  and `adapters/zsh/fuxx-autosuggestions.plugin.zsh` exists); `published_crates_carry_licences_and_readme`
  (in `core/`, `fuxx-autosuggestions/`, `cli/`: `README.md` exists, `LICENSE-MIT` and
  `LICENSE-APACHE` are byte-equal to the root's); `fuxx_autocomplete_is_not_published`
  (`fuxx-autocomplete/Cargo.toml` has `publish = false`).
- [x] **Step 2:** run (`cargo test -p fuxx-completes --test release`): fail (no pins, no copies,
  no `publish = false`).
- [x] **Step 3:** `git mv projects core/projects`; `build.rs` reads `$CARGO_MANIFEST_DIR/projects`
  (and its comments; `core/src/projects/mod.rs` comments); the workspace metadata, pins and
  per-crate `readme`; the licence copies; three short crate READMEs (what it is, that it is
  macOS-first and pre-1.0, a link to the repo and `docs/design.md`); `publish = false` in
  `fuxx-autocomplete`. Run: the four tests pass.
- [x] **Step 4: Verify the packages build from their tarballs.** Run `cargo publish --workspace
  --exclude fuxx-autocomplete --dry-run` (no upload). Expected: `Packaging`, `Verifying` and
  `aborting upload due to dry run` for core, then fuxx-autosuggestions, then fuxx-completes; no
  `projects/: No such file`. If cargo refuses because the core isn't on crates.io yet, run
  `cargo package -p fuxx-completes-core` alone (must verify) and record that the other two can
  only be verified during Gate D. `cargo package --list -p <each>` shows `LICENSE-MIT`,
  `LICENSE-APACHE`, `README.md` (core: `projects/*.toml`). Remove `target/package/`.
- [x] **Step 5:** gate. **Docs:** design §12 (`core/projects/`; crate files), §5.4's mapping paths,
  `projects/README.md` moves along; roadmap Progress log. Commit `chore: crates ready for crates.io (mappings inside the core, metadata, licences)`.

### Task 2: Both release zips, by script

**Interfaces — Consumes:** Task 1's version agreement. **Produces:** `scripts/package.sh
[--allow-dirty]` → `target/dist/v<version>/{fuxx-plugin-macos-arm64.zip,
fuxx-plugin-macos-x86_64.zip, SHA256SUMS}`, exit 0 and silent on success, a one-line reason and
exit 1 on any failed check.

- [x] **Step 1: The notices.** `THIRD_PARTY_NOTICES.md` "Rust crates in the binary": each crate,
  its version from `Cargo.lock`, its upstream, "used under Apache-2.0 (see `LICENSE-APACHE`)",
  and the copyright line from its own licence file in `~/.cargo/registry/src/` when it has one.
  The Fig section stays reserved.
- [x] **Step 2: Write `scripts/package.sh`** per the AI's call (expected zip entries, in this
  order of checks: clean tree, versions, build both targets with `--locked`, `lipo -archs` equals
  `arm64` / `x86_64`, size < 786,432, `codesign -v` on arm64, stage, `ditto` zip, `zipinfo -1`
  equals exactly `LICENSE-APACHE LICENSE-MIT THIRD_PARTY_NOTICES.md bin/ bin/fuxx-completes
  fuxx-plugin.conf shell/ shell/fuxx-autosuggestions.plugin.zsh` (sorted; no `__MACOSX`, no `._`),
  `ditto -x -k` into `mktemp -d`, `bin/fuxx-completes` is executable and its `doctor` (x86_64
  under `arch -x86_64`) prints `fuxx-completes <version>` with a temp `HOME`, temp folders removed
  by a `trap`).
- [x] **Step 3: Prove each check bites.** With `--allow-dirty` on a scratch edit: set
  `fuxx-plugin.conf`'s version to `9.9.9` → `version mismatch: fuxx-plugin.conf 9.9.9, Cargo.toml
  <v>`, exit 1; restore. Without `--allow-dirty` on a dirty tree → `uncommitted changes`, exit 1.
  Then a clean run: exit 0, both zips and `SHA256SUMS` in `target/dist/v<version>/`.
- [x] **Step 4: `x86_64_suite_passes_under_rosetta`.** `cargo test --workspace --target
  x86_64-apple-darwin` (the test binaries and the core run under Rosetta; the shells stay the
  arm64 builds in `target/`). Expected: the same pass count as the native run. And
  `cargo test --release -p fuxx-completes --target x86_64-apple-darwin --test size -- --ignored
  --nocapture`: passes, the size recorded. A failure here is debugged (systematic-debugging), not
  skipped; over the limit → stop and ask the user (as M1d did).
- [x] **Step 5:** gate (no adapter touched). **Docs:** design §8 (both zips real; what they hold,
  with the licence files; how they are made), §3 (x86_64 size measured), §14 (the Rosetta run);
  roadmap. Commit `feat: scripts/package.sh makes both release zips (arm64, x86_64)`. Then
  remove `target/dist/` and `target/x86_64-apple-darwin/`.

### Task 3: The Homebrew formula, checked locally

**Interfaces — Consumes:** Task 2's zips. **Produces:** `scripts/formula.sh <version> [--local]`
printing `Formula/fuxx-completes.rb` to stdout: `class FuxxCompletes < Formula`, `desc "Ghost-text
suggestions for zsh and bash from your own history"` (≤ 80 chars, no leading article, per `brew
audit`), `homepage`, `url`/`sha256` of the tag tarball, `license any_of: ["MIT", "Apache-2.0"]`,
`depends_on :macos`, `on_arm`/`on_intel` `resource "binary"`, `install`, `caveats`, `test` per the
AI's calls. It refuses a version that differs from `Cargo.toml`'s.

- [x] **Step 1:** write `scripts/formula.sh`. `--local` builds `target/dist/v<v>/fuxx-completes-<v>.tar.gz`
  with `git archive --prefix=fuxx-completes-<v>/ HEAD` and uses `file://` URLs to it and the zips.
- [x] **Step 2: The local tap check** (Homebrew's own tree only; removed at the end):
  `HOMEBREW_NO_AUTO_UPDATE=1 brew tap-new --no-git local/fuxx-check`; write the `--local` formula
  into it; `brew install --formula local/fuxx-check/fuxx-completes`; `brew test
  local/fuxx-check/fuxx-completes`; `brew audit --strict local/fuxx-check/fuxx-completes`;
  `brew style local/fuxx-check/fuxx-completes`. Expected: install puts `fuxx-completes` in
  `$(brew --prefix)/bin` and `zsh/`, `bash/`, `fish/` under `$(brew --prefix)/share/fuxx-completes/`;
  test passes; audit and style show nothing but the `file://` URL complaints (recorded). If audit
  rejects the prebuilt route outright, switch to the build-from-source fallback, say so in the
  Progress log, and tell the user (it needs Homebrew's `rust`).
- [x] **Step 3:** run the caveats' zsh line in a temp-HOME `zsh -d -i` (as `adapters/zsh/README.md`'s
  try-it shell, with a copy of a generated history): `fuxx-completes doctor` shows `state on` and
  the binary from Homebrew's `bin`. Then `brew uninstall fuxx-completes`, `brew untap
  local/fuxx-check`; `brew list` and `brew tap` show neither.
- [x] **Step 4:** gate. **Docs:** design §15 "Left to the builder" → a line on how each channel is
  packaged (formula route, where adapters land, caveats, no `vendor_conf.d`); roadmap. Commit
  `feat: scripts/formula.sh renders the Homebrew formula from a release`.

### Task 4: Install docs that work, and the load shapes they rely on

**Interfaces — Consumes:** the zsh harness (`cli/tests/common/zsh.rs`), the adapter's
`_FUXX_AS_FILE`, `_FUXX_AS_SECOND`, `doctor` facts.

- [x] **Step 1: Failing tests** (`zsh_adapter.rs`, the zsh matrix): `loads_through_plugin_manager_shapes`
  (Review Focus 3: the adapter copied into `$HOME/My Plugins/zsh/`, linked as
  `$HOME/.oh-my-zsh/custom/plugins/fuxx-autosuggestions` → sourced as
  `…/fuxx-autosuggestions/fuxx-autosuggestions.plugin.zsh` from inside a function `load_plugin(){
  source "$1"; }`; ghost text for `git s` shows `tatus`; `_FUXX_AS_FILE` is the resolved real path;
  `source ~/.zshrc` again keeps one core and no `_FUXX_AS_SECOND`); `brew_copy_and_fuxx_copy_one_runs`
  (Review Focus 2: `.zshrc` sources copy A from `$HOME/brew/share/fuxx-completes/zsh/`; then, as
  fuxx does after `.zshrc`, copy B from `$HOME/Library/Application Support/fuxx/plugins/x/shell/`
  with `FUXX_PLUGIN_DATA` set while sourcing → one core; `doctor` shows A as running and B as
  stayed off; the context file lands under `XDG_DATA_HOME`, not B's data folder). If either
  passes at once, keep it as a guard and say so; no adapter change is planned.
- [x] **Step 2:** run; fix only what fails (systematic-debugging first); zsh matrix.
- [x] **Step 3: The README, made real.** Status line (what's released, what isn't: the menu);
  "Install" replaces "Planned install": **Homebrew** (`brew install fitzfitz/fuxx/fuxx-completes`,
  then the caveats' line for your shell); **zsh plugin managers** (the five lines of the AI's call,
  each saying the binary comes from Homebrew or `cargo install fuxx-completes`); **bash**, **fish**
  (R5); **fuxx** (Settings → Plugins, paste `github.com/fitzfitz/fuxx-completes`); **from
  crates.io** (`cargo install fuxx-completes` for the binary; the crates for embedders); **not**
  a browser download of the zip (macOS quarantines it and blocks the unsigned binary); upgrading
  and uninstalling (`brew upgrade`/`uninstall`; `fuxx-completes forget` first to wipe the data);
  "Shell support (planned)" becomes what is built. `adapters/zsh/README.md`: drop the stale "M0,
  waiting for a check" and "Stays off (M1, not built yet) … none of that is detected" text; Install
  section points at the README. `adapters/bash/README.md`, `adapters/fish/README.md`: Install gains
  the Homebrew path.
- [x] **Step 4: Manual check of each manager** (real machine, temp `HOME` and `ZDOTDIR`, network
  only to clone public repos, everything under one `mktemp -d` removed after): oh-my-zsh,
  zinit, antidote, zap and plain `source`, each with the README's exact lines and
  `FUXX_COMPLETES_BIN` set to the Task 2 arm64 binary; `fuxx-completes doctor` says `state on`
  each time. Record the result per manager in the Progress log; a line that doesn't work is fixed
  in the README, not worked around in the adapter. Ghost text on screen is the user's check
  (Task 6, Gate B).
- [x] **Step 5:** gate + zsh matrix. **Docs:** design §5.6 (how managers load the file, no root
  file), §8 (the sample `description` updated to the shipped one and the `setting.*` lines noted,
  since they drifted), §15 status; roadmap. Commit `docs: install for Homebrew, zsh plugin managers, bash, fish, fuxx and crates.io`.

### Task 5: Version, review, fix pass

- [ ] **Step 1:** set the version (R1; `0.1.0` with the AI's pick) in `[workspace.package]`, both
  pins and `fuxx-plugin.conf`; `cargo build` updates `Cargo.lock`. `versions_agree` passes.
- [ ] **Step 2:** all three matrices; `scripts/package.sh` clean run (then remove `target/dist/`).
  Design status line and §15 row: "built and reviewed, waiting for the release gates"; roadmap row.
- [ ] **Step 3:** one whole-branch review (superpowers:requesting-code-review, most capable model,
  this Review Focus, plus: does any step publish without a gate; do the README's lines match what
  Task 3 and Task 4 actually ran). One fix pass, each fix test-first where testable; gate and
  matrices again. Commit `version 0.1.0` and the fixes separately.

### Task 6: The release, one gate at a time

Each gate: the builder shows exactly what will be published, waits for the user's word, does it,
verifies, records it in the Progress log. A "no" stops the gate; nothing after it runs.

- [ ] **Gate A — merge and push `main`.** Merge `m3-release` into `main`,
  push `main` to `github.com/fitzfitz/fuxx-completes` (already public). *Published:* the branch's
  commits. *Undo:* a revert commit; history stays public.
- [ ] **Gate B — tag `v0.1.0` and the GitHub release.** On `main`: `scripts/package.sh`; `git tag
  v0.1.0`; push the tag; `gh release create v0.1.0 target/dist/v0.1.0/fuxx-plugin-macos-*.zip
  --title "v0.1.0 — fuxx-autosuggestions" --notes-file <notes>` (notes drafted from the README:
  what it does in zsh, bash, fish; install lines; macOS 11+ on Apple Silicon and Intel; what isn't
  there yet). Re-download both zips with `gh release download` into a temp folder; their sha256
  must equal `SHA256SUMS` and GitHub's `digest`. *Published:* the tag, the release, two zips;
  **this becomes "latest", so every fuxx with the plugin is offered the update.** *Undo:* `gh
  release delete v0.1.0 --cleanup-tag`; fuxx installs already made stay. Then **the user's check
  in fuxx** (Settings → Plugins → update; ghost text, the list, Settings; design §15 "installed
  and checked in fuxx"); on an Intel Mac too if the user has one (R4). A problem found here is
  fixed as `0.1.1` through Tasks 5–6, never by replacing assets.
- [ ] **Gate C — the tap (R3).** `gh repo create fitzfitz/homebrew-fuxx --public` with
  `Formula/fuxx-completes.rb` (`scripts/formula.sh 0.1.0`, real URLs) and a short README (what it
  is, `brew install fitzfitz/fuxx/fuxx-completes`). *Published:* a new public repo. *Undo:* delete
  the repo; anyone who tapped keeps a stale clone until `brew untap`. Then, on this Mac:
  `brew install fitzfitz/fuxx/fuxx-completes`, `brew test`, `brew audit --strict --online
  fitzfitz/fuxx/fuxx-completes` (clean), the caveats' zsh line in a temp-HOME shell → `doctor`
  `state on`; `brew uninstall`, `brew untap`.
- [ ] **Gate D — crates.io (R2), last.** The user runs `cargo login` (their token, their account).
  `cargo publish --workspace --exclude fuxx-autocomplete --dry-run`, then without `--dry-run`.
  *Published:* `fuxx-completes-core 0.1.0`, `fuxx-autosuggestions 0.1.0`, `fuxx-completes 0.1.0`,
  permanently. *Undo:* `cargo yank` (the version stays downloadable for lockfiles and can never be
  reused); deletion only within 72 h, or with one owner, < 1000 downloads a month and no
  dependents (delete the dependents first). Then `cargo install fuxx-completes --root <temp>` →
  `doctor` prints `fuxx-completes 0.1.0`; remove the temp root.
- [ ] **Close:** design status and §15 row, roadmap row "✅ done" only on the user's word after
  Gate B's check; Progress log per gate. Remove every `target/dist/`, temp folder and
  `target/x86_64-apple-darwin/`.

## Self-review (2026-11-23)

- Spec coverage (§15 M3 row): Homebrew tap (T3, Gate C); zsh plugin manager install (T4); crates.io
  (T1, Gate D); README install section made real (T4); `fuxx-plugin.conf` and both
  `fuxx-plugin-macos-*.zip` assets (T2, Gate B); installed and checked in fuxx (Gate B). §3
  platforms (macOS, both chips, size limit: T2); §8 zip contents and data folder (T2, T4); §12
  layout (T1); §13 nothing sent, no rc edits, no `vendor_conf.d` (T3); §14 (T2's Rosetta run, T4's
  tests); THIRD_PARTY_NOTICES and licences (T1, T2).
- Types: `field()` (T1) is test-only; `package.sh` and `formula.sh` read the same
  `[workspace.package]` version and refuse any other; `target/dist/v<version>/` is the one
  hand-off between T2, T3 and Gate B.
- Not in M3 (design): Linux, the menu (M6–M7), `fuxx-autocomplete` on crates.io (M8), fuxx drawing
  over the host channel, `init` and a demo (R6), bottles, notarisation (no Developer ID in the
  design).
