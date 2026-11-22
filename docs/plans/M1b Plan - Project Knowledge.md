# M1b Project Knowledge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Written 2026-11-22. The user: "just go ahead with your best recommendation pick" — so this plan is
executed at once, natively (executing-plans), with one whole-branch review at the end. Decisions
marked **AI's call** are open to change; each lands in `docs/design.md` in the task named.

**Goal:** In a project folder, the ghost text offers that project's usual commands (`cargo test`,
`pnpm dev`, `make <target>`) when this folder's own history has no match, within the 10 ms budget
and at most 150 KB of extra binary.

**Architecture:** Mapping data in `projects/*.toml` is turned into static tables by `core/build.rs`.
`core/src/projects/` holds six minimal readers, detection (walk up the folders, one listing each,
match markers, read small files), and an in-memory cache filled by a worker thread.
`fuxx-autosuggestions::suggestion()` gains the three-step order: this folder's history, then project
candidates, then the rest of history. The protocol and the zsh adapter don't change.

**Tech Stack:** Rust 2021 (1.97.1). New **build-only** dependencies: `toml = "1.2"` and
`serde = { version = "1", features = ["derive"] }` (Context7 `/websites/rs_toml_toml`:
`toml::from_str::<T: Deserialize>`); no new runtime dependency.

**Spec:** `docs/superpowers/specs/2026-11-21-project-knowledge-design.md` (the M1b spec) and
`docs/design.md` §2, §3, §5.4, §5.8, §12, §15. Rules: `AGENTS.md`.

## Decisions this plan settles (spec §8 "left to the builder", and gaps)

| Question | **AI's call** | Why |
| --- | --- | --- |
| TOML shape | `[[project]]` with `id`, `markers` (any of; a name or `*.ext`), `requires` (all of; relative paths), `requires_any`, `contains = { file, text }`, `beats`, `commands`, and `[[project.from]]` with `reader`, `files` (first that exists is read; for `file_names` a `*.ext` pattern), `path`, `templates` (`{}` = the name) | Covers every ecosystem below (flutter needs `contains`, android needs `requires_any`, compose has four file names) |
| Mapping order | Files in `projects/` by name, entries in file order; `build.rs` panics with the file and entry on bad data (unknown reader, unknown `beats` id, duplicate id, a template without `{}`) | Deterministic order; bad data fails the build, never the user |
| What "this folder's history" is (step 1) | Matches with a context record in the cwd that are **not failed-only** there (tier 1, and neutral-only such as ⌃C'd ones); a typo that only failed here doesn't hide project commands. A shell with no usable data folder has no step 1 | M1's approved rule keeps failed-only last; the context index already holds this session's reports |
| Order inside step 1 | Tier-1 order of M1, then neutral-only-here ones by recency | M1's ranking unchanged |
| Exact match | A project candidate equal to the typed text is skipped (the next longer one may show) | Typing a whole command shouldn't end project help |
| Walk budget | Steps 1 and 3 share M0's one 50 ms budget and 50-candidate bound | Two walks never double the worst case |
| Walk-up | From the cwd up to 8 folders; stop **after** a folder holding `.git`; stop **before** `$HOME` and `/` (they are never scanned) | Projects live in repos; a stray file in `$HOME` must not make every folder a project |
| Reading | Regular files only (never a FIFO or device: opening one could block), at most 256 KB, read as lossy UTF-8 | Spec §4; a FIFO named `package.json` would hang the worker |
| Cache | 64 folders, least recently used dropped; an entry is fresh while every scanned folder's (mtime, size) and every read file's (mtime, size) are unchanged; a stale entry is still served while it is re-detected | Spec §4–§5.2; a few `stat`s per request |
| Worker | One detection in flight per session (a request for another cold folder while busy gets none this time); 50 ms cap, checked between folders and files; on the cap the partial result is cached | Bounded threads, never blocks an answer |
| Name safety | Extracted names 1–100 chars of `A-Za-z0-9-_.:/@+`; others dropped; fixed commands from our data are trusted | Spec §5.4 |
| Duplicates | A candidate string seen earlier (nearer project, earlier entry) is dropped | One ghost text per command |
| Size check | `cli/tests/size.rs` (`#[ignore]`, release): the binary must stay under **656,400 bytes** (M1's 502,800 + 150 KB) | The budget is a test, not a promise |

### Ecosystems (the first set; spec §5.5). Fixed commands are the everyday ones; "→" is the reader

| id | Detected by | Beats | Commands (first ones) | Reader → templates |
| --- | --- | --- | --- | --- |
| npm | `package.json` | | `npm install`, `npm test` | json_keys `package.json` `scripts` → `npm run {}` |
| pnpm | `pnpm-lock.yaml` + `package.json` | npm | `pnpm install` | scripts → `pnpm {}`, `pnpm run {}` |
| yarn | `yarn.lock` + `package.json` | npm | `yarn install` | scripts → `yarn {}` |
| bun | `bun.lock`/`bun.lockb` + `package.json` | npm | `bun install` | scripts → `bun run {}` |
| deno | `deno.json`/`deno.jsonc` | | `deno task` | json_keys `tasks` → `deno task {}` |
| expo | `app.json` containing `"expo"` | | `npx expo start` | |
| uv | `uv.lock` | pip | `uv sync`, `uv run` | toml_keys `pyproject.toml` `project.scripts` → `uv run {}` |
| poetry | `poetry.lock` | pip | `poetry install` | toml_keys `tool.poetry.scripts` → `poetry run {}` |
| pip | `requirements.txt` | | `pip install -r requirements.txt` | |
| django | `manage.py` | | `python manage.py runserver`, `… migrate`, `… makemigrations`, `… test` | |
| cargo | `Cargo.toml` | | `cargo build`, `cargo test`, `cargo run`, `cargo check`, `cargo clippy`, `cargo fmt` | toml_keys `Cargo.toml` `bin[].name` → `cargo run --bin {}`; `.cargo/config.toml` `alias` → `cargo {}` |
| cmake | `CMakeLists.txt` | | `cmake -B build`, `cmake --build build` | |
| cmake-presets | `CMakePresets.json` | | | json_keys `configurePresets[].name` → `cmake --preset {}`; `buildPresets[].name` → `cmake --build --preset {}` |
| meson | `meson.build` | | `meson setup build`, `meson compile -C build`, `meson test -C build` | |
| make | `Makefile`/`GNUmakefile`/`makefile` | | `make` | make_targets → `make {}` |
| dotnet | `*.sln`/`*.csproj`/`*.vbproj`/`*.fsproj` | | `dotnet build`, `dotnet run`, `dotnet test`, `dotnet restore` | |
| flutter | `pubspec.yaml` containing `sdk: flutter` | dart | `flutter run`, `flutter test`, `flutter pub get`, `flutter build apk`, `flutter build ios` | |
| dart | `pubspec.yaml` | | `dart run`, `dart test`, `dart pub get` | |
| android | `gradlew` + any of `app/build.gradle`, `app/build.gradle.kts` | gradlew, gradle | `./gradlew assembleDebug`, `./gradlew installDebug`, `./gradlew test`, `./gradlew lint`, `./gradlew clean` | |
| gradlew | `gradlew` + any of `build.gradle`, `build.gradle.kts`, `settings.gradle`, `settings.gradle.kts` | gradle | `./gradlew build`, `./gradlew test`, `./gradlew clean` | |
| gradle | `build.gradle`/`build.gradle.kts` | | `gradle build`, `gradle test` | |
| maven | `pom.xml` | | `mvn clean install`, `mvn test`, `mvn package` | |
| xcode | `*.xcodeproj`/`*.xcworkspace` | | | file_names `*.xcodeproj` → `xcodebuild -scheme {} build`, `xcodebuild -scheme {} test`, `open {}.xcodeproj`; `*.xcworkspace` → `open {}.xcworkspace` |
| swiftpm | `Package.swift` | | `swift build`, `swift test`, `swift run` | |
| cocoapods | `Podfile` | | `pod install`, `pod update` | |
| go | `go.mod` | | `go build ./...`, `go test ./...`, `go run .`, `go mod tidy` | |
| ruby | `Gemfile` | | `bundle install`, `bundle exec` | |
| rails | `Gemfile` + `bin/rails` | | `bin/rails server`, `bin/rails console`, `bin/rails db:migrate`, `bin/rails test` | |
| composer | `composer.json` | | `composer install` | json_keys `scripts` → `composer {}` |
| laravel | `artisan` | | `php artisan serve`, `php artisan migrate`, `php artisan tinker`, `php artisan test` | |
| just | `justfile`/`Justfile`/`.justfile` | | `just` | just_recipes → `just {}` |
| task | `Taskfile.yml`/`Taskfile.yaml` | | `task` | yaml_keys `tasks` → `task {}` |
| compose | `compose.yaml`/`compose.yml`/`docker-compose.yml`/`docker-compose.yaml` | | `docker compose up`, `docker compose up -d`, `docker compose down`, `docker compose logs -f` | yaml_keys `services` → `docker compose up {}`, `docker compose logs -f {}` |
| docker | `Dockerfile` | | `docker build .` | |
| terraform | `*.tf` | | `terraform init`, `terraform plan`, `terraform apply`, `terraform fmt` | |
| helm | `Chart.yaml` | | `helm lint .`, `helm template .` | |
| mix | `mix.exs` | | `mix deps.get`, `mix test`, `mix compile`, `iex -S mix` | |
| zig | `build.zig` | | `zig build`, `zig build test`, `zig build run` | |
| nix | `flake.nix` | | `nix build`, `nix develop`, `nix run`, `nix flake update` | |

## Global Constraints

- Suggestion answers under **10 ms at p95** (design §3), project candidates included; detection
  never runs on the answering path (worker thread, **50 ms** cap).
- The feature adds at most **150 KB** to the release binary: under **656,400 bytes** (M1 left
  502,800). No new runtime dependency.
- **Never run anything** in a project; only list folders and read regular files of at most
  **256 KB**. Nothing in a project adds a reader or a template.
- Cache: **64** folders, memory only; nothing about projects is written to disk (design §13).
- Walk at most **8** folders up.
- Extracted names: 1–100 chars of `A-Za-z0-9-_.:/@+`.
- Tests use temp folders and checked-in fixtures only; never the user's folders.
- Per-task gate: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --check`. Docs (design + roadmap Progress log) with every task. Commit on
  branch `m1b-project-knowledge`.

## Review Focus

1. **A hostile or odd repo:** a script named `rm -rf ~;`, one with ESC or a newline, a 50 MB
   `package.json`, a FIFO named `package.json`, a symlink loop: names dropped, file skipped, never
   a hang. Tests in Tasks 2–3.
2. **A huge or slow folder** (100,000 entries, the cwd is `/` or `$HOME`): detection hits its cap
   or never scans; an answer never waits. Tests in Tasks 3–4.
3. **A folder that changes** (`npm init`, an edited `scripts`, a deleted `Makefile`): the next
   request after re-detection shows the new commands, not stale ones forever. Test in Task 4.
4. **Monorepo:** a package inside a repo with its own `package.json`: the package's scripts come
   first; the repo root still contributes. Test in Task 3.
5. **History and projects disagree:** `cargo tree` typed elsewhere, `cargo t` here → `cargo test`;
   `git st` here with no project `git` command → the user's `git status`; a failed-only typo here
   doesn't hide project commands. Tests in Task 5.

---

## File map

| File | What it holds |
| --- | --- |
| `projects/*.toml` (new) | the mapping data above, grouped by family (`node.toml`, `python.toml`, …) |
| `core/build.rs` (new) | reads `projects/*.toml`, checks it, writes `$OUT_DIR/projects_table.rs` |
| `core/src/projects/mod.rs` (new) | `Marker`, `Reader`, `Source`, `ProjectType`, `PROJECTS` (the generated table), re-exports |
| `core/src/projects/readers.rs` (new) | the six readers, `safe_name`, `read_small` |
| `core/src/projects/detect.rs` (new) | `detect`, `Detection`, `Stamp` |
| `core/src/projects/cache.rs` (new) | `ProjectCache` |
| `core/tests/fixtures/projects/<id>/` (new) | one fixture folder per ecosystem |
| `core/tests/projects.rs` (new) | the every-ecosystem fixture test |
| `core/src/history.rs` | `ranked_here` |
| `fuxx-autosuggestions/src/lib.rs` | the three-step `suggestion` |
| `cli/src/session.rs` | owns a `ProjectCache`, passes candidates |
| `cli/tests/latency.rs`, `cli/tests/size.rs` (new) | cold/warm latency, the size budget |

---

### Task 1: Mapping data and the generated table

**Files:** create `projects/*.toml`, `core/build.rs`, `core/src/projects/mod.rs`; modify `core/Cargo.toml` (build-dependencies), `core/src/lib.rs`.

**Interfaces — Produces:**
- `pub enum Marker { Name(&'static str), Ext(&'static str) }` (`Ext(".csproj")` for `*.csproj`)
- `pub enum Reader { JsonKeys, TomlKeys, MakeTargets, JustRecipes, YamlKeys, FileNames }`
- `pub struct Source { pub reader: Reader, pub files: &'static [&'static str], pub path: &'static str, pub templates: &'static [&'static str] }`
- `pub struct Contains { pub file: &'static str, pub text: &'static str }`
- `pub struct ProjectType { pub id: &'static str, pub markers: &'static [Marker], pub requires: &'static [&'static str], pub requires_any: &'static [&'static str], pub contains: Option<Contains>, pub beats: &'static [&'static str], pub commands: &'static [&'static str], pub from: &'static [Source] }`
- `pub static PROJECTS: &[ProjectType]`

- [ ] **Step 1:** Context7 check for `toml`/`serde` (done in planning: `toml` 1.2, `from_str` needs default features).
- [ ] **Step 2: Failing tests** in `projects/mod.rs`: `table_has_every_planned_ecosystem` (the set of the 39 ids above); `ids_are_unique_and_beats_are_known`; `templates_hold_a_placeholder`; `pnpm_entry_reads_package_json_scripts` (reader `JsonKeys`, files `["package.json"]`, path `scripts`, templates `["pnpm {}", "pnpm run {}"]`); `xcode_markers_are_extensions` (`Ext(".xcodeproj")`, `Ext(".xcworkspace")`).
- [ ] **Step 3:** run `cargo test -p fuxx-completes-core projects` — fails (no module).
- [ ] **Step 4:** write the data and `build.rs` (serde structs mirroring the TOML; `cargo::rerun-if-changed=../projects`; emit Rust literals with `{:?}` for strings; panic on bad data as in the decisions table); `include!` the output in `mod.rs`.
- [ ] **Step 5:** tests pass; gate. **Docs:** design §5.4 (the data shape, bad data fails the build), §12. Commit `feat(core): project mappings as data, compiled into static tables`.

### Task 2: The six readers

**Files:** create `core/src/projects/readers.rs`.

**Interfaces — Produces:**
- `pub fn safe_name(s: &str) -> bool`
- `pub fn read_small(path: &Path) -> Option<String>` (regular file ≤ 256 KB, lossy UTF-8; never opens a non-regular file)
- `pub fn json_keys(text: &str, path: &str) -> Vec<String>` — `a.b` = keys of the object there; `a[].k` = the string value of `k` in each element of array `a`; JSONC comments and trailing commas tolerated; keys in file order, duplicates dropped
- `pub fn toml_keys(text: &str, path: &str) -> Vec<String>` — `a.b` = keys in table `[a.b]`; `a[].k` = `k = "…"` in each `[[a]]`; bare and quoted keys
- `pub fn make_targets(text: &str) -> Vec<String>` — column-0 rule lines (`name … :` but not `:=`/`::=`/`?=`), no `%`, no `$`, not `.`-leading
- `pub fn just_recipes(text: &str) -> Vec<String>` — column-0 recipe headers (`@` prefix dropped), not `_`-leading; skips `alias`, `set`, `export`, `import`, `mod`, `[attr]` and `:=` lines
- `pub fn yaml_keys(text: &str, key: &str) -> Vec<String>` — block style: the keys at the first child indent under column-0 `key:`; comments and blank lines skipped; flow style (`{…}`) gives none
- every reader returns only `safe_name` names

- [ ] **Step 1: Failing tests:** `safe_name_rules` (`dev`, `build:prod`, `@scope/pkg`, `a+b` ok; ``, 101 chars, `rm -rf ~;`, `a b`, `x$y`, `"q"`, `a\u{1b}`, `a\nb` refused); `json_keys_scripts_in_order` (`{"scripts":{"dev":"x","build":"y"}}` → `["dev","build"]`); `json_keys_nested_and_jsonc` (comments, trailing comma, a nested object value skipped, escaped quotes in keys); `json_keys_array_names` (`{"configurePresets":[{"name":"debug"},{"name":"rel"}]}` with `configurePresets[].name` → `["debug","rel"]`); `json_keys_unsafe_names_dropped`; `toml_keys_table` (`[project.scripts]` → keys until the next header); `toml_keys_array_of_tables` (two `[[bin]]` → their `name`s); `make_targets_rules` (`build:`, `test: build`, `a b: c` → `a`,`b`; skips `.PHONY:`, `%.o:`, `VAR := 1`, `$(X):`, tab-led recipe lines); `just_recipes_rules` (`build:`, `@test arg:`, `_hidden:`, `alias b := build`, `set shell := …`, `x := "1"`); `yaml_keys_block` (`tasks:\n  build:\n    cmds: []\n  test:\n` → `["build","test"]`, nested keys ignored, next column-0 key ends it); `read_small_skips_big_and_non_regular` (a 300 KB file → `None`; a FIFO made with `mkfifo` → `None` within 100 ms; a folder → `None`).
- [ ] **Steps 2–4:** run (fail), implement (hand-written scanners, no dependencies), run (pass); gate.
- [ ] **Step 5 docs:** design §5.4 readers' exact rules. Commit `feat(core): six minimal project readers with name safety`.

### Task 3: Detection

**Files:** create `core/src/projects/detect.rs`, `core/tests/fixtures/projects/<id>/…` (one per id), `core/tests/projects.rs`.

**Interfaces — Consumes:** Tasks 1–2. **Produces:**
- `pub struct Stamp { pub path: PathBuf, pub mtime: (i64, i64), pub len: u64 }`
- `pub struct Detection { pub candidates: Vec<String>, pub stamps: Vec<Stamp>, pub ids: Vec<&'static str> }` (`ids` nearest first)
- `pub fn detect(cwd: &Path, home: Option<&Path>, deadline: Instant) -> Detection`

Rules: the walk-up of the decisions table; per folder one `read_dir`; a type matches when a marker is in the listing, every `requires` exists (in the listing, or by `stat` for paths with `/`), one `requires_any` exists if listed, and `contains.text` is in `contains.file` (`read_small`); then a matched type's `beats` drop other matched types **in the same folder**; candidates per project = `commands`, then each source's names through each template (first template first); folders nearest first; duplicates dropped; stamps for every listed folder and read file; past the deadline the walk stops with what it has.

- [ ] **Step 1: Failing tests** (`detect.rs`, temp folders): `cargo_project_offers_cargo_commands`; `pnpm_beats_npm` (package.json + pnpm-lock.yaml, scripts `dev` → `pnpm install`, `pnpm dev`, `pnpm run dev`; no `npm run dev`); `nearest_project_first_in_a_monorepo` (repo root with `.git` + package.json scripts `lint`; `packages/app/package.json` scripts `dev`; from `packages/app`: `npm run dev` before `npm run lint`); `walk_stops_after_git_and_before_home`; `walk_is_at_most_8_levels`; `deadline_in_the_past_gives_nothing_quickly`; `unreadable_parent_stops_quietly`; `hostile_names_and_files` (scripts `rm -rf ~;` and `ok`; a FIFO named `Makefile` → returns within 200 ms with `npm run ok`, no `rm` candidate); `stamps_cover_listed_folders_and_read_files`.
- [ ] **Step 2:** `core/tests/projects.rs`: `every_ecosystem_has_a_fixture_and_is_detected` — for each `PROJECTS` entry, `core/tests/fixtures/projects/<id>/` exists (holding a `.git` file so the walk stops there); `detect(fixture)` has `id` in `ids`; the entry's first fixed command (if any) is a candidate; for each source, every template applied to `fx` is a candidate (each reader-backed fixture defines one item named `fx`).
- [ ] **Steps 3–5:** run (fail), implement, run (pass); gate. **Docs:** design §5.4 (walk-up rule, `$HOME` and `/` never scanned, beats per folder). Commit `feat(core): project detection with fixtures for every ecosystem`.

### Task 4: The cache and the worker

**Files:** create `core/src/projects/cache.rs`.

**Interfaces — Produces:** `pub struct ProjectCache`; `fn new() -> Self`; `fn candidates(&mut self, cwd: &Path, home: Option<&Path>) -> &[String]` (fresh entry: served; stale or missing: served if present, and a detection starts unless one is in flight; missing: empty); `pub const MAX_FOLDERS: usize = 64; pub const DETECT_BUDGET: Duration = Duration::from_millis(50);` and `pub fn wait_idle(&mut self)` (joins the in-flight detection; tests and the benchmark).

- [ ] **Step 1: Failing tests:** `cold_folder_answers_empty_then_fills`; `fresh_entry_is_served_without_detecting_again`; `a_changed_script_is_picked_up`; `a_deleted_makefile_drops_its_targets`; `stale_entry_is_served_while_redetecting`; `lru_keeps_64_folders`; `one_detection_in_flight`; `candidates_call_is_fast_when_warm` (1,000 warm calls < 50 ms total in debug).
- [ ] **Steps 2–4:** run (fail), implement, run (pass); gate. **Docs:** design §3 (cache and worker rules). Commit `feat(core): project cache filled off the typing path`.

### Task 5: The three-step order, the session, latency and size

**Files:** modify `core/src/history.rs`, `fuxx-autosuggestions/src/lib.rs`, `cli/src/session.rs`, `cli/tests/latency.rs`; create `cli/tests/size.rs`.

**Interfaces — Produces:**
- `History::ranked_here<'a>(&'a self, prefix: &str, ranking: &Ranking) -> Vec<&'a str>` (step 1 set and order, per the decisions table; empty without a secret or index)
- `pub fn suggestion<'a>(history: &'a History, buffer: &str, place: &Place, ranking: &Ranking, projects: &'a [String]) -> Option<&'a str>`: step 1 = walk `ranked_here`; step 2 = first project candidate starting with `buffer` and longer than it; step 3 = walk `ranked_with_prefix` minus the step-1 set; steps 1 and 3 share one budget
- `Session` holds a `ProjectCache` and passes `candidates(&cwd, home)` to `suggestion`

- [ ] **Step 1: Failing tests** (fuxx-autosuggestions): `this_folders_history_beats_project_commands`; `project_command_beats_history_from_elsewhere`; `history_from_elsewhere_still_shows_without_a_project_match`; `failed_only_here_does_not_hide_project_commands`; `neutral_here_counts_as_this_folders_history`; `exact_project_command_is_skipped_for_a_longer_one`; the M1 tests pass with `&[]`. Session: `a_cold_project_folder_fills_in_on_a_later_request`.
- [ ] **Step 2:** `cli/tests/latency.rs`: a project folder (package.json with 200 scripts, a Makefile with 200 targets, Cargo.toml) — report the cold first request and assert warm p95 < 10 ms. `cli/tests/size.rs` (`#[ignore]`): the release binary is < 656,400 bytes.
- [ ] **Steps 3–5:** run (fail), implement, run (pass, both `--release -- --ignored` runs); gate. **Docs:** design §5.4 (the three steps as built), §5.8 (the exception), §3 (measured numbers, binary size before/after). Commit `feat: project candidates after this folder's history, before the rest`.

### Task 6: Whole-branch check and the user's check

- [ ] Full gate, `scripts/test-zsh-matrix.sh`, both release benchmarks; one whole-branch review (most capable model); one fix pass; again.
- [ ] Test release (as v0.0.6/v0.0.7: tagged on the branch, `main` untouched) when the user asks; the user's check in a few real project folders; then M1b ✅, merge, roadmap.

## Self-review (2026-11-22)

- Spec coverage: data + build.rs (T1), six readers + name safety (T2), detection, walk-up,
  beats, ordering, every ecosystem fixture (T3), cache 64 + worker + 50 ms + stale-while-revalidate
  (T4), the three-step order, cold/warm latency, size budget (T5). Non-goals respected: nothing
  run, no user mappings, nothing on disk.
- Types: `ProjectType`/`Source`/`Reader`/`Marker` (T1) used by T3; `Detection`/`Stamp` (T3) by T4;
  `ProjectCache::candidates` (T4) by T5. Signatures match.
