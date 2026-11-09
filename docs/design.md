# fuxx-completes: design

Status: **design stage.** Nothing here is built. This document is the source of truth for the
repo; `docs/roadmap.md` tracks the milestones it defines.

Written 2026-11-09 from a brainstorm with the user. Every decision below is the user's unless it is
marked **AI's call**. An AI's call is a reasoned default, open to change by the user.

---

## 1. Intent

Two helpers for the plain shell prompt, built on one engine of our own:

- **fuxx-autosuggestions:** grey ghost text after the cursor that completes the current command
  from commands you ran before. → accepts all of it; a word key accepts part of it.
- **fuxx-autocomplete:** a menu while you type: commands, subcommands, flags, arguments, git
  branches, files.

### Why

The good versions of these exist, but each is tied to one shell (zsh-autosuggestions,
zsh-autocomplete, fish's built-ins) or was a closed app that is gone (Fig). The user wants one
engine that:

- works the same in zsh, bash and fish, in any terminal;
- does not depend on zsh's completion system ("we dont ever rely on zsh");
- can be embedded by other apps, and by fuxx, the user's GPU terminal for AI agents
  (github.com/fitzfitz/fuxx-cli), which can draw a better menu than a shell can.

## 2. Goals, non-goals, anti-goals

### Goals

1. Ghost-text suggestions from history in zsh first, then bash and fish where the shell allows.
2. An as-you-type menu of commands, subcommands, flags, arguments and live values.
3. One Rust core that holds all the logic. Adapters only draw and forward keys.
4. Our own command knowledge, seeded from Fig's MIT-licensed specs.
5. The core is usable from other programs (as a Rust crate, or through the local protocol).
6. fuxx uses the core directly and can switch each library on or off.

### Non-goals (not planned)

- Suggestions inside AI agents (Claude Code and similar). They have their own. Plain prompt only.
- History sync, a history database or a replacement for ⌃R search (that is atuin's job).
- AI or LLM-generated suggestions. Not in the plan.
- Windows, PowerShell, nushell, or other shells beyond zsh, bash and fish.
- Network access of any kind. See §13.
- A general shell framework, prompt theme, or plugin manager.

### Anti-goals (must never happen)

- **Breaking zsh's own Tab completion.** If the user has `compinit` and their own completions,
  those keep working.
- **Breaking ⌃R** or any history search the user already has (zsh's built-in, fzf, atuin).
- **Breaking prompt themes** such as powerlevel10k (including its instant prompt) or Starship.
- **Slowing down typing.** A keystroke must never wait on the core. A slow core means a missing
  suggestion, never a slow prompt.
- **Changing the user's history** or writing copies of it anywhere.
- **Fighting another plugin** that already does the same job. See §9.

## 3. Constraints

### Latency budget (AI's call)

These are targets for the builder to measure against, not measurements.

| Path | Target |
| --- | --- |
| Core answers a suggestion request | under 10 ms at p95, on a history of 100,000 lines |
| Core answers a menu request without generators | under 30 ms at p95 |
| A generator (e.g. git branches) | runs off the typing path; default timeout 500 ms; cached |
| Keystroke handling in the adapter | never waits for the core at all (async) |

The 10 ms figure is there so the ghost text keeps up with a fast typist. If a request misses its
budget the adapter shows nothing for that keystroke; it does not wait.

### Never block the shell

- Adapters send a request and return to the line editor at once. The answer is drawn when it
  arrives (zsh: a file-descriptor handler, `zle -F`).
- Every request carries an id. An answer for an older buffer is dropped.
- If the core process is missing, crashed or hung, the adapter goes quiet for that session. It
  never prints errors at the prompt. A `doctor` command (§7) explains what happened.
- Adapters print nothing during shell startup. (powerlevel10k's instant prompt treats console
  output during initialization as a problem.)

### Async fetch

History loads once per core process, then stays in memory and is updated by the adapter's
"command ran" reports. Generators run in background threads or child processes with a timeout,
and their results are cached by (generator, working directory) for a short time. A menu shows
static items first and adds generator items when they arrive.

## 4. Decided (the user's decisions, as decided)

- **One repo, two libraries:** fuxx-autosuggestions and fuxx-autocomplete.
- **Our own core engine in Rust**, shared by both. It does history reading and ranking, command
  knowledge, and generators for live values. "We dont ever rely on zsh."
- **Our own command knowledge.** It does not borrow zsh's completion system. It starts from
  Fig's withfig/autocomplete specs (MIT), converted into our own format, with the original
  license notice kept. From then on the data is ours to maintain.
- **Thin adapters, no smarts:** zsh, bash and fish plugins, other terminal apps, and fuxx.
- **fuxx talks to the core directly,** can draw a nicer menu, and switches each library on or off
  in its Preferences.
- **Users:** people on zsh, bash or fish in any terminal (Ghostty, iTerm, Warp and others), apps
  that embed the core, and fuxx.
- **Scope:** the plain shell prompt only, not inside AI agents.
- **Order:** autosuggestions first (small, mostly history) to prove the core and the adapters.
  Autocomplete after.
- **Repo policy:** one repo for this family. Libraries with a different job get their own `fuxx-…`
  repo under the same GitHub account. No GitHub org. The umbrella is the `fuxx-` name prefix, the
  GitHub topic `fuxx`, and a "Libraries" list in fuxx's README.
- **Timing:** building starts after fuxx's v0.5.0 release. The fuxx side (switches, loading the
  adapters, the nicer menu) becomes a milestone in fuxx's own roadmap later, not here.
- **License (AI's call):** MIT OR Apache-2.0, the Rust convention.
- **Layout (AI's call):** a Cargo workspace (§12).

## 5. Architecture

```
  zsh plugin     bash plugin     fish plugin     other apps          fuxx
  (draws only)   (draws only)    (draws only)    (draw their own)    (draws a nicer menu)
       \              |              /               |                  |
        \             |             /                |                  |
         +---- fuxx-completes binary ----------------+                  |
               (local protocol, §5.5)                                   |
                      |                                                 |
        +-------------+-------------------------------------------------+
        |                    Rust crates (linked directly by fuxx)
        v
  fuxx-autosuggestions        fuxx-autocomplete
        \                        /
         +-- fuxx-completes-core +
             history · ranking · line parsing · specs · generators · protocol types
```

### 5.1 The core (`core/`, crate `fuxx-completes-core`)

Holds every piece of logic that is not drawing:

- **History.** Reads the shell's history file read-only, once, and keeps it in memory.
  Supported formats: zsh (plain and `EXTENDED_HISTORY`, `: <start>:<elapsed>;<command>`, with
  zsh's metafied bytes decoded), bash (plain, and `#<epoch>` timestamp lines when
  `HISTTIMEFORMAT` is set), fish (`~/.local/share/fish/fish_history`). The adapter tells the core
  which file and format; reading a file format is parsing, not depending on a shell. The adapter
  also reports each command as it runs (with its working directory and exit status), which the
  file alone doesn't record. **AI's call:** no database of our own in the first milestones; the
  shell's file plus in-memory session records are enough to start, and nothing is copied to disk.
- **Line parsing.** Splits the typed line to find the current command and the word under the
  cursor: quotes, escapes, `|`, `;`, `&&`, `||`, and the start of `$(` and backticks. When it can't
  tell, it returns no answer rather than a wrong one. The adapter may pass a dialect hint (zsh,
  bash, fish) for the few syntax differences.
- **Command knowledge.** Loads our specs (§5.2).
- **Generators.** Runs live-value lookups (§5.3).
- **Ranking.** Orders history matches and menu items (§5.4).
- **Protocol types.** The messages of §5.5, shared by the binary and by embedders.

The core never calls, embeds or depends on a shell at runtime, and it has no network code.

### 5.2 The spec format

Our own declarative format describing a command:

- name(s) and description;
- subcommands (recursive);
- options: names (`-v`, `--verbose`), description, whether it takes an argument, whether it can
  repeat, which options exclude it;
- arguments: name, optional or required, variadic, and where values come from: a fixed list, a
  template (`files`, `folders`), or a named generator.

One file per command under `specs/`. Whether files are JSON or TOML, and whether they are compiled
into the binary or loaded from disk, is settled in the first autocomplete plan (M4). Specs never
contain code. Fig's `postProcess` and `custom` JavaScript functions don't carry over (§11).

### 5.3 Generators

A generator produces live values: git branches, git remotes, files in a folder, npm scripts from
`package.json`, and similar. Two kinds:

- **Built-in** generators written in Rust in the core, referenced by name (`git.branches`).
- **Declarative** generators in a spec: a program and its arguments to run, plus how to split and
  trim its output.

Rules:

- Run the program directly with an argument list, never through `sh -c` or any shell.
- Only generators from specs we ship (or the user's own spec folder) run. Nothing in the current
  directory or a cloned repo can add one.
- Timeout (default 500 ms, AI's call), then give up quietly.
- Cache per (generator, working directory) for a short time; the TTL is the builder's choice.
- Never on the keystroke path: the menu shows other items first.

### 5.4 Ranking

**Suggestions** start with parity with zsh-autosuggestions' default `history` strategy: the most
recent history entry that starts with the typed text. Then, in later milestones, context from the
session records: same working directory, the command that ran just before, and whether it
succeeded. The weights are the builder's choice; ranking must be deterministic for a given history
and context, so it can be tested.

**Menu items** are ordered by kind (subcommands, then options, then argument values; open to
change), then by prefix match quality, then by how often the user picked them (counted in memory
only, unless a later plan decides otherwise).

### 5.5 The adapter protocol (AI's call, a proposal to settle in the first plan)

The shell plugins can't link a Rust crate, so they talk to a small binary, `fuxx-completes`.

Proposal:

- One long-lived `fuxx-completes serve` child process per shell session, started by the adapter on
  the first prompt. It loads history once and keeps it warm. Starting a process on every keystroke
  would reload history each time and can't meet the budget.
- Requests and responses are **line-delimited JSON** over the child's stdin and stdout, one object
  per line. Every request has an `id`; every response echoes it.
- Shell code shouldn't have to parse JSON. The binary may also offer a plain field-separated
  encoding of the same messages for the shell adapters (for example fields split by a control
  character, newlines escaped). Settle which in the first plan.
- Messages, roughly:
  - `hello` (shell, version, history file and format, which libraries are on) → `ready`
  - `suggest` (buffer, cursor, cwd) → `suggestion` (text to show after the buffer, or none)
  - `complete` (buffer, cursor, cwd) → `items` (label, insert text, kind, description), possibly
    followed by more `items` when generators finish
  - `ran` (command, cwd, exit status) → no reply
  - `bye`
- Transport detail for zsh: zsh allows only one `coproc` at a time and other plugins may use it,
  so the adapter should open its own pipe pair instead (named pipes in a private temp folder, or
  a Unix socket via `zmodload zsh/net/socket`). The builder picks one in M0.
- A shared per-user daemon on a Unix socket would save memory across many shells but adds
  lifecycle work (start, stop, stale sockets), and bash can't open a socket without an extra
  tool. Deferred until per-session processes prove too heavy.

Where the binary's code lives (a `cli/` crate, or a `[[bin]]` in an existing crate) is settled in
the M0 plan.

Embedders: other Rust programs link the crates directly; programs in other languages run the
binary and speak JSON. fuxx links the crates (§8).

### 5.6 The adapters

Adapters are small shell scripts in `adapters/<shell>/`, one plugin file per library so each can be
turned on alone (for example `fuxx-autosuggestions.plugin.zsh` and `fuxx-autocomplete.plugin.zsh`).
They:

- start the core process and send it requests;
- draw the answer with the shell's line editor;
- bind the accept keys;
- report each command that ran;
- detect conflicting plugins and stay off (§9).

They hold no ranking, parsing or command knowledge.

## 6. How each shell shows ghost text and the menu

Honest about what each shell allows.

### zsh (the first and best-supported adapter)

- **Ghost text:** set ZLE's `POSTDISPLAY` to the part of the suggestion after the buffer, and add a
  `region_highlight` entry from `$#BUFFER` to `$#BUFFER + $#POSTDISPLAY` with a dim style (default
  `fg=8`, configurable). This is how zsh-autosuggestions does it. Where available (zsh 5.9+), tag
  our entry with `memo=` so we only ever remove our own entries; other plugins such as
  zsh-syntax-highlighting also write `region_highlight`.
- **Widget wrap:** like zsh-autosuggestions, wrap the widgets that edit the buffer so that after
  each edit we request a new suggestion, and wrap the accept widgets. By default: `forward-char`
  (→) and `end-of-line` (End) at the end of the buffer accept the whole suggestion;
  `forward-word` (⌥→ / Esc f) accepts up to where the cursor moves. Other widgets clear or keep it.
  Wrapping happens at the first `precmd`, so widgets defined by plugins loaded after us are seen.
- **Async:** the answer arrives on a file descriptor watched with `zle -F`; the handler sets
  `POSTDISPLAY` and redraws (`zle -R`).
- **Hooks:** use `add-zle-hook-widget` and `add-zsh-hook` instead of overwriting `zle-line-init`,
  `precmd` or `preexec`, so powerlevel10k and others keep theirs. `preexec` (or `zshaddhistory`)
  reports the command; `precmd` reports the exit status.
- **Menu:** zsh has no popup. The menu is drawn below the line, either as extra lines in
  `POSTDISPLAY` styled with `region_highlight`, or with `zle -M`. We don't use `compadd` or the
  completion system to draw it, because that would tie the menu to zsh's completion machinery.
  Keys to move through and pick items are bound in a keymap of our own while the menu is open.
  Which drawing method holds up best (long lists, narrow terminals, multi-line prompts) is
  settled in M6.
- **Tab:** we don't take Tab by default. zsh's own completion stays on Tab.
- **Limits:** a menu below the line pushes the screen and can't overlap other text; very long
  buffers and multi-line prompts need testing.

### bash

- **Plain readline has no ghost text hook.** There is no `POSTDISPLAY` equivalent and no
  per-keystroke hook. Drawing grey text with escape codes behind readline's back breaks on redraw,
  so we won't.
- **What plain bash can do:** `bind -x` runs a shell function on a key and lets it read and set
  `READLINE_LINE` and `READLINE_POINT` (bash 4.0+). So a key can insert the current best
  suggestion, and a key can show a menu (printed below, then the prompt redrawn). Not
  as-you-type.
- **macOS ships bash 3.2,** which lacks `READLINE_LINE`. The bash adapter requires bash 4 or later
  (e.g. Homebrew's) and says so.
- **ble.sh** replaces readline with a line editor written in bash and has its own grey
  auto-complete. When ble.sh is loaded, whether we can feed our suggestions into it through its
  own interfaces is an open question for the bash milestone. If we can't, ble.sh users keep its
  built-in feature and we stay off.
- **Tab:** bash's `complete` is left alone by default.

### fish

- **fish already has autosuggestions,** from history and its completions, on by default, and
  offers no public hook to supply our own ghost text. So fuxx-autosuggestions **stays off in
  fish**: fish's own version is good and two ghost texts can't share one line. The fish adapter
  may still report commands, so ranking stays consistent if the user also uses fuxx.
- **Menu:** fish's `complete -c <command> -a '(…)'` can call our binary for argument values, and
  fish shows them in its own pager on Tab. That uses fish's completion display, not as-you-type.
  fish's own completions keep priority unless the user turns ours on for a command.
- **Limits:** no as-you-type menu in fish. Stated plainly in the README.

### Other terminal apps and fuxx

They get structured answers (JSON, or Rust types) and draw however they like: a popup at the
cursor, a ghost-text overlay. They still need to know the buffer and cursor, which only the shell
has, so in practice they pair with a shell adapter that forwards them, or with shell integration
of their own.

## 7. Configuration and the doctor

Small and later. Each adapter reads a few shell variables (on/off per library, highlight style,
accept keys). `fuxx-completes doctor` prints what is loaded, which conflicting plugins were found,
the shell and version, and the core's last error. No config DSL. Details belong in the M1 plan.

## 8. How fuxx uses it

- fuxx links `fuxx-autosuggestions` and `fuxx-autocomplete` as Rust crates; no protocol, no child
  process.
- Preferences gets one switch per library.
- fuxx loads the shell adapters into its own terminals (the same way it already injects a small
  zsh integration through `ZDOTDIR` for command blocks, without touching dotfiles) so it can know
  the buffer and cursor, then draws a nicer menu itself.
- All of this is fuxx's work and becomes a milestone in fuxx's roadmap after its v0.5.0 release.
  This repo only has to keep the crate APIs usable for an embedder.

## 9. Coexisting with other plugins (AI's call)

Rule: **detect it, and stay off.** Never fight another tool for the same keys or the same space on
the line.

| Found | What we do |
| --- | --- |
| zsh-autosuggestions (e.g. `_zsh_autosuggest_start` defined) | fuxx-autosuggestions stays off in that shell |
| zsh-autocomplete (its `.autocomplete__*` functions) | fuxx-autocomplete stays off |
| fzf key bindings (`fzf-history-widget`, `fzf-file-widget`, `fzf-completion`) | we never bind ⌃R, ⌃T or Tab; if `fzf-completion` holds Tab, our menu never uses Tab |
| atuin | we never bind ⌃R or ↑ |
| fish's own autosuggestions | fuxx-autosuggestions off in fish (§6) |
| ble.sh auto-complete | stay off unless the bash milestone finds a clean way in |

Detection runs at the first `precmd` (zsh) or first prompt, after the user's rc file has loaded
everything. Staying off is silent at the prompt and shown by `doctor`. A user can force ours on.

## 10. Prior art

- **zsh-autosuggestions:** fish-like ghost text in zsh via `POSTDISPLAY`, `region_highlight` and
  widget wrapping; strategies `history`, `completion` and `match_prev_cmd`; an async mode. Our zsh
  adapter's drawing follows its proven approach.
- **zsh-autocomplete:** a real-time completion listing in zsh, built on zsh's completion system
  (`compinit`, compsys). Shows how good as-you-type can feel; we differ by not using compsys.
- **Fig / Amazon Q Developer CLI:** a popup menu at the cursor in many terminals, driven by
  open-source TypeScript specs (withfig/autocomplete, MIT). Fig was acquired by Amazon in 2023 and
  became part of Amazon Q Developer CLI. Its specs are our starting data.
- **atuin:** replaces shell history with a SQLite database, optional encrypted sync, and a search UI
  on ⌃R. A different job; we coexist with it.
- **carapace:** a multi-shell completion engine in Go with its own spec format and bridges to
  other completers. The closest in spirit to our core; worth studying before the autocomplete plan.
- **fish's built-ins:** autosuggestions from history and completions, and completions generated
  from man pages. The bar for how it should feel.

## 11. Importing Fig specs

- **Source:** github.com/withfig/autocomplete, `src/`, one TypeScript file per command, each
  exporting a `Fig.Spec`: `name`, `description`, `subcommands`, `options` (with `name`, `args`,
  `isRepeatable`, `exclusiveOn` and more), `args` (with `isOptional`, `isVariadic`, `template`
  such as `"filepaths"`/`"folders"`, `suggestions`, `generators`). `pnpm build` compiles them to
  JavaScript in `build/`.
- **Conversion:** a one-off converter (a dev tool run by maintainers, not something users install)
  loads the compiled specs and writes our format. Declarative parts convert directly. Generators
  with a `script` and simple splitting convert to our declarative generators. `postProcess`,
  `custom` and other JavaScript functions can't convert; those generators are dropped, or rewritten
  by hand as built-in Rust generators for the commands that matter most (git first).
- **Which commands:** start with a short list (git and a handful of common tools), not all of them.
  Start small.
- **Pinning:** record the upstream commit converted from.
- **License:** MIT. Converted files keep the upstream copyright and permission notice in
  `THIRD_PARTY_NOTICES.md`, copied exactly from upstream's LICENSE at import time, with the commit.
  The data is ours to change from then on.
- **Where:** `specs/` (see `specs/README.md`), with imported files marked as such.
- Whether the converter uses Node (to run Fig's build) is the builder's call in M4; it is a
  maintainer tool, not a dependency of the libraries.

## 12. Repo layout (AI's call)

```
Cargo.toml                 workspace
core/                      fuxx-completes-core
fuxx-autosuggestions/      the ghost-text library
fuxx-autocomplete/         the menu library
specs/                     command knowledge (our format; imported data marked)
adapters/zsh|bash|fish/    thin shell plugins
docs/design.md             this document
docs/roadmap.md            milestones
THIRD_PARTY_NOTICES.md     licenses of imported data
```

Licensed MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).

## 13. Privacy

- History and command lines **never leave the machine.** The core has no network code, no
  telemetry, no crash reporting that uploads.
- History files are read, never written. No copies are saved. Session records live in memory.
- Generators run local programs only.
- Logs never contain command lines by default. A debug log, if any, is local, opt-in and says so.
- fuxx passes the buffer to the crates in-process; it doesn't go anywhere else.

## 14. Testing strategy

- **Core unit tests:** history parsing for each format (including zsh metafied bytes, multi-line
  commands, bash timestamps, fish's file), ranking with fixed histories, line parsing edge cases,
  spec loading.
- **Golden tests:** Fig conversion input → expected output for a few specs.
- **Protocol tests:** start the binary, send requests, check responses and ids, check stale answers
  are dropped and a hung generator times out.
- **Latency benchmarks:** suggestion and menu requests against a synthetic 100,000-line history;
  fail when the budget in §3 is exceeded.
- **Adapter tests:** drive a real zsh (then bash, fish) in a pseudo-terminal with `zsh -f` or
  equivalent, a **temporary HOME** and `ZDOTDIR`, and a generated history file. Never the user's
  real rc files or history.
- **Coexistence tests:** load zsh-autosuggestions, zsh-autocomplete, fzf's bindings and
  zsh-syntax-highlighting in the temp shell; check we stay off or don't break them; check Tab and
  ⌃R still do what they did.
- **Manual checks** (can't be automated well; say so): ghost text and menu look right in Ghostty,
  iTerm, Terminal.app and fuxx, with powerlevel10k's instant prompt, in a narrow window.

## 15. Milestones

| | Milestone | Done when |
| --- | --- | --- |
| M0 | Core + history + zsh adapter | the binary serves suggestions from a zsh history file; the zsh plugin shows ghost text, → accepts, ⌥→ accepts a word; async; temp-HOME tests pass |
| M1 | Autosuggestions: context and coexistence | ranking uses cwd, previous command and exit status; conflicting plugins detected; `doctor`; latency benchmark meets §3 |
| M2 | bash and fish adapters (autosuggestions) | bash 4+: key-driven accept, ble.sh path decided; fish: stays off cleanly, reports commands |
| M3 | Release fuxx-autosuggestions | Homebrew tap, zsh plugin manager install, crates.io; README install section made real |
| M4 | Spec format + Fig import | format settled; converter; git and a few common tools imported; notices in place |
| M5 | Line parsing + generators | current command and word found; built-in and declarative generators with timeout and cache |
| M6 | zsh menu | as-you-type menu in zsh drawn below the line; Tab untouched; fzf-safe |
| M7 | bash and fish menus | key-driven menu in bash; fish argument values through `complete` |
| M8 | Release fuxx-autocomplete | packaged like M3 |

The fuxx side (Preferences switches, loading adapters, the nicer menu) is a milestone in fuxx's
roadmap, not here.

### Left to the builder

- The transport and encoding details of §5.5, the binary's home, and crate dependencies (each
  checked through Context7 first).
- Ranking weights, cache TTLs, the spec file format (JSON or TOML).
- How the zsh menu is drawn (§6), and the ble.sh question.
- Packaging details for each install channel.

## 16. Risks (recorded honestly)

- **Fig needed a funded team and still got acquired.** A full product of this kind is a lot of
  work, and it didn't survive alone.
- **Command knowledge needs ongoing upkeep.** CLIs change every release; imported specs go stale.
- **One maintainer with AI help.** Time is the limit.
- **Start small.** Autosuggestions in zsh first; a short spec list; no feature before it's needed.
- Shell limits are real: bash and fish will never match zsh here (§6).

## 17. Open questions

1. Line-delimited JSON for everyone, or a simpler encoding for shell adapters (§5.5)?
2. Can ble.sh take our suggestions through its own interfaces (§6)?
3. JSON or TOML for specs, compiled in or loaded from disk (§5.2)?
4. Should menu pick counts persist across sessions, and if so where (§5.4)?
5. Which commands make the first spec list beyond git (§11)?
