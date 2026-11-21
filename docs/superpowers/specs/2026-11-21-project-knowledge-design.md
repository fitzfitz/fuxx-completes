# fuxx-completes: frequency ranking and project knowledge — design

**Status:** approved by the user 2026-11-21, saved to pick up later. Folded into `docs/design.md`
(§2, §3, §4, §5.4, §5.8, §12, §15) and `docs/roadmap.md` (M1, new M1b). Not built. The
implementation plans (`superpowers:writing-plans`) are written when M1 and then M1b are reached;
M0 is still waiting for the user's check in a real terminal. Section numbers below are this
document's own; "design §n" means `docs/design.md`.

## 1. Intent and why

The user asked whether fuxx-autosuggestions saves the commands they run most. It doesn't: today
the ghost text is the newest history entry starting with the typed text (`core/src/history.rs:58`),
with the `cd`/`pushd` rules of design §5.4 on top. They want two things:

1. **Frequency.** Commands they run most rank higher, except `cd` and `open` (the user's call).
2. **Project knowledge.** fuxx-completes knows what kind of project a folder is (node, uv, cargo,
   C++, VB, flutter, android, macOS and more) and suggests that project's usual commands, even
   ones never typed there (`npm run dev` from `package.json`, `cargo test`, `flutter run`). In the
   user's words: "With these libraries mapping on our end, it should make ours smarter, but always
   pay more attention on performance for size and the fast suggestions is needed."

**Success:** in a project folder they've never worked in, typing `cargo t` or `pnpm d` shows the
right command at once; in a folder they know, their own habits win; the binary stays small and the
ghost text keeps its 10 ms budget (design §3).

## 2. Goals, non-goals, anti-goals

**Goals**
- Frequency in the folder is a ranking factor for history suggestions (M1).
- Project-type mappings for the ecosystems in §5.5, as data compiled into the binary.
- Project commands fill in the ghost text **only when this folder's history has no match** for
  the typed text (the user's decisions, 2026-11-21: "history first", then "this folder's history":
  a match typed only in other folders does not hide the project's commands).
- The same readers later feed the menu's generators (design §5.3, M5), so they are built once.

**Non-goals**
- Running anything in the project to learn about it (`gradle tasks`, `npm run`, `make -p`, …).
- User-written or downloaded mappings. Only the mappings we ship. (A later change, if wanted.)
- A second helper program in another language. The Rust core already is the fast, light helper
  (a 525 KB binary held in memory per shell); a second process would only add IPC. **AI's call,
  answering the user's "if we must we can make our own helper with other technologies".**
- Saving anything about projects to disk. The project cache is memory only.

**Anti-goals**
- A project command showing when the user's history **in this folder** has a match for the typed
  text.
- The ghost text slowing down: cold detection in a new folder never delays an answer.
- A cloned repo making us run its code, or garbling the terminal through odd names in its files.
- The binary growing noticeably: the whole feature has a size budget (§4).

## 3. Approaches (the user picked 1)

1. **Mappings as data + a few built-in readers — picked.** Each project type is a short data entry:
   marker files, fixed commands, and which reader extracts project-specific ones. Readers are a
   handful of Rust parsers shared across ecosystems. New types are mostly data.
2. Rust code per project type: no format to design, but 25+ near-copies, and every new type is code.
3. Inside the Fig-derived spec format (M4): one format, but M4 isn't designed; would pull it ahead.

Why 1: as fast and small as 2 (the data is turned into static Rust tables at build time), but
adding a type later is a few lines of data.

## 4. Constraints

- **Speed.** A suggestion request stays under 10 ms at p95 (design §3), including project
  candidates. Detecting and parsing a folder's projects happens on a worker thread; the first
  request in a never-seen folder may answer from history only while it runs, later requests use
  the cache. A folder's detection has a time cap (**AI's call: 50 ms**) and gives up quietly.
- **Size.** The whole feature adds at most **150 KB** to the release binary (**AI's call**),
  measured before and after. The core has no runtime dependencies today; prefer hand-written
  minimal readers (they only pull a few keys out of a file) over general parsers; a dependency is
  allowed only if it fits the budget and is checked through Context7 first.
- **Data compiled in.** Mapping data lives as TOML under `projects/` in the repo and is turned
  into static tables by `build.rs` (a build-time dependency costs no binary size).
- **Read only, never run.** Detection lists folders and reads small files. Files over **256 KB**
  are skipped (**AI's call**). Nothing in the project can add a reader or a command template.
- **Memory.** The cache holds a bounded number of folders (**AI's call: 64**, least recently used
  dropped).
- **Privacy** (design §13): nothing about projects is written to disk or sent anywhere.

## 5. How it works

### 5.1 Frequency (part of M1)
M1's context file (design §5.7) already records, per run, a keyed hash of the command, the folder,
the exit code and the time. Ranking adds **how many times the command ran successfully in this
folder** as a factor next to recency, cwd and exit status (weights stay the builder's, ranking
stays deterministic). **`cd`, `pushd` and `open` get no frequency boost** (the user's call): they
keep recency and the existing `cd` rules. Only the first simple command's word is looked at
(after `builtin`/`command` and `NAME=value`, like the `cd` check).

### 5.2 Detecting projects
From the request's cwd, walk up folder by folder to the first of: a folder holding `.git`, `$HOME`,
or `/`; at most **8 levels** (**AI's call**). In each folder, **one directory listing** (not a
`stat` per marker) is matched against all marker names and patterns (`*.csproj`, `*.xcodeproj`).
Every project found is kept, **nearest folder first**, so a monorepo package's own scripts come
before the root's. The cache key is the folder; an entry is reused while the listing's and each
read file's (mtime, size) are unchanged (a few `stat`s per request).

### 5.3 A mapping entry (the data format; final shape left to the builder)
```toml
[[project]]
id = "pnpm"
markers = ["pnpm-lock.yaml"]          # any of these present
requires = ["package.json"]
beats = ["npm"]                       # when both match, npm's entry is dropped
commands = ["pnpm install", "pnpm dev", "pnpm test", "pnpm build"]
[[project.from]]
reader = "json_keys"                  # one of the built-in readers
file = "package.json"
path = "scripts"
templates = ["pnpm {}", "pnpm run {}"]
```
Package-manager choice comes from lockfiles through `beats` (pnpm/bun/yarn over npm, uv/poetry
over plain pip).

### 5.4 Readers (built-in, Rust)
| Reader | Pulls | Used by |
|---|---|---|
| `json_keys` | keys of an object at a path | package.json scripts, deno.json tasks, composer.json scripts, CMakePresets.json presets |
| `toml_keys` | keys of a table | pyproject `[project.scripts]`, `[tool.poetry.scripts]`, Cargo `[[bin]]` names, `.cargo/config.toml [alias]` |
| `make_targets` | rule targets (no `%` patterns, no `.`-leading) | Makefile, GNUmakefile |
| `just_recipes` | recipe names (no `_`-leading private ones) | justfile |
| `yaml_keys` | keys under one top-level key (block style only) | Taskfile.yml `tasks:`, compose.yaml `services:` |
| `file_names` | names of files/folders matching a pattern | `*.xcodeproj`/`*.xcworkspace` (scheme = name), `*.sln`/`*.csproj`/`*.vbproj` |

JSON/JSONC/TOML/YAML readers only need to find keys, not parse every value; a file they can't read
gives no commands, never an error.

**Name safety:** an extracted name is used only if it is 1–100 characters of letters, digits and
`-_.:/@+` (no spaces, quotes, `$`, control or wire characters); anything else is dropped rather
than quoted (**AI's call**).

### 5.5 Ecosystems (first set; the user may strike any)
User's list: **node** (npm/pnpm/yarn/bun), **uv**, **cargo**, **C++** (CMake, CMakePresets, Meson,
Make), **VB** (VB.NET through `dotnet`; classic VB6 is out, **AI's call**), **flutter**
(`pubspec.yaml` with a flutter dependency → `flutter run/test/pub get/build apk/build ios`; plain
Dart → `dart run/test`), **android** (`gradlew` + `app/build.gradle*` → `./gradlew assembleDebug/
installDebug/test/lint/clean`), **macOS** (`*.xcodeproj`/`*.xcworkspace` → `xcodebuild -scheme <name>
build/test`, `open <name>.xcodeproj`; `Package.swift` → `swift build/test/run`; `Podfile` →
`pod install`).

AI's additions: deno, poetry, pip (`requirements.txt`), Django (`manage.py runserver/migrate/
makemigrations/test`), Go, Maven, Gradle (non-Android), .NET C#/F#, Ruby/Rails, PHP/Composer/
Laravel `artisan`, Expo (`app.json` with `expo` → `npx expo start`), Make, just, Task, Docker
Compose, Dockerfile, Terraform, Helm, Elixir/mix, Zig, Nix flakes.

### 5.6 Ranking (the user's decisions: history first, and that means this folder's history)
"This folder's history" = commands with at least one record in the context file (design §5.7) or
in this session's reports (§5.8) whose folder is the request's cwd (**AI's call:** the cwd exactly,
not its parents or the project root).

1. History matches with a record in this folder (ranked by M1: recency, frequency, exit status)
   → the best one is shown; project commands are not consulted.
2. Otherwise, project candidates that start with the typed text: nearest project first, then the
   entry's `commands` in listed order, then reader results in file order (first template first).
   The first match is shown.
3. Otherwise, the rest of history as today (newest first, with the `cd` rules) — matches typed
   only in other folders.

Example: `cargo tree` typed once in another folder, a new Rust project, `cargo t` → `cargo test`
(step 2). Typing `git st` in the same project still shows the user's `git status` from elsewhere
(step 3), because no project offers a `git` command.

Once the user runs a project command it is in this folder's history and ranks in step 1 from then
on, which is how the ghost text "learns" the project.

A project candidate is never shown when the typed text is empty (same as history). The `cd`
rules don't apply to project candidates (none are `cd`).

## 6. Where it lives
- `core/src/projects/` — detection, cache, readers, candidate list (the core stays shell-free).
- `projects/*.toml` — mapping data; `core/build.rs` turns it into static tables.
- `fuxx-autosuggestions` asks the core for project candidates when history has none.
- The protocol (design §5.5) is unchanged: the request already carries the cwd.

## 7. Milestones
- **Frequency** joins **M1** (the next milestone; a §5.4 addition).
- **Project knowledge** is a new milestone **M1b** after M1, before M2 (**AI's call**). It
  **depends on M1**: step 1 of §5.6 needs the context file's folder records. Its size budget is
  measured from the binary as M1 leaves it. Done when: detection, cache and
  the six readers with temp-folder tests; every ecosystem in §5.5 has a fixture folder and a test
  (the full list stands unless the user strikes some; **AI's call** after the builder check);
  the three-step order of §5.6 is tested; latency measured within §3 with a cold
  and a warm folder; binary growth measured under 150 KB.

## 8. Left to the builder
The exact TOML shape and `build.rs` output; the readers' internals; frequency weights; the
command lists inside each ecosystem entry (kept short: the everyday ones); worker-thread details.

## 9. Changes to the design (made 2026-11-21)
- §2 Goals: add project knowledge. §2 Non-goals: running project code, user mappings.
- §4 Decided: frequency except `cd`/`open`; project knowledge; this folder's history first, then
  project commands, then other history; approach 1 (data + readers).
- §5.4 Ranking: frequency factor; the three-step order of §5.6 here.
- §5.8: a stated exception — project candidates are the one thing suggested that isn't from
  history (only when this folder's history has no match).
- §3: the cold-folder rule and the size budget. §12: `projects/`. §15 + roadmap: M1 line, new M1b.

## Verification (when built)
`cargo test --workspace` (readers and detection on temp fixture folders, ranking order tests);
release binary size before/after; the latency benchmark on a warm and a cold folder; then the
user's check in a real terminal in a few real project folders.
