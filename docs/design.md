# fuxx-completes: design

Status: **design stage.** Nothing here is built. This document is the source of truth for the
repo; `docs/roadmap.md` tracks the milestones it defines.

Written 2026-11-09 from a brainstorm with the user, and revised the same day after a builder check
(facts corrected, gaps filled). Every decision below is the user's unless it is marked **AI's
call**. An AI's call is a reasoned default, open to change by the user.

**Changed 2026-11-13 (the user's decision, in fuxx):** fuxx no longer links these crates. fuxx
gained plugins (fuxx's note "76 - Milestones 11y-11z - Plugins", approved 2026-11-13), and
**fuxx installs fuxx-completes as a fuxx plugin** like anyone else's. §2 goal 6, §4, §8, §9 and
§15 are updated for it.

---

## 1. Intent

Two helpers for the plain shell prompt, built on one engine of our own:

- **fuxx-autosuggestions:** grey ghost text after the cursor that completes the current command
  from commands you ran before. → accepts all of it; a word key accepts part of it (§6).
- **fuxx-autocomplete:** a menu while you type: commands, subcommands, flags, arguments, git
  branches, files.

### Why

The good versions of these exist, but none is what the user wants:

- Most are tied to one shell: zsh-autosuggestions, zsh-autocomplete, fish's built-ins.
- Fig worked across shells, but as a closed app. Amazon announced its acquisition in August 2023,
  and the Fig app shut down on 1 September 2024; its line lives on as Amazon Q Developer CLI (§10).
- Multi-shell engines do exist: carapace (completions, in Go) and Microsoft's inshellisense
  (as-you-type, MIT, built on Fig's specs). inshellisense is the closest prior art; §10 says
  honestly how we differ.

The user wants one engine that:

- works the same in zsh, bash and fish, in most terminals (not Warp, whose own input editor
  replaces the shell's: §9);
- does not depend on zsh's completion system ("we dont ever rely on zsh");
- can be embedded by other apps, and by fuxx, the user's GPU terminal for AI agents
  (github.com/fitzfitz/fuxx-cli), which can draw a better menu than a shell can.

## 2. Goals, non-goals, anti-goals

### Goals

1. Ghost-text suggestions from history in zsh first, then bash and fish where the shell allows.
2. An as-you-type menu of commands, subcommands, flags, arguments and live values.
3. One Rust core that holds all the logic. Adapters only draw, forward keys and forward facts.
4. Our own command knowledge, seeded from Fig's MIT-licensed specs.
5. The core is usable from other programs (as a Rust crate, through the local protocol, or as a
   host terminal over the host channel of §5.5).
6. fuxx installs fuxx-completes as a **fuxx plugin** and can switch it on or off (§8; changed
   2026-11-13, was "fuxx uses the core directly").

### Non-goals (not planned)

- Suggestions inside AI agents (Claude Code and similar). They have their own. Plain prompt only.
- History sync, a copy of the command history, or a replacement for ⌃R search (that is atuin's
  job). The small context file of §5.7 holds no command text and is not a history database.
- AI or LLM-generated suggestions. Not in the plan.
- Windows, PowerShell, nushell, or other shells beyond zsh, bash and fish.
- Network access of any kind. See §13.
- A general shell framework, prompt theme, or plugin manager.

### Anti-goals (must never happen)

- **Breaking zsh's own Tab completion.** If the user has `compinit` and their own completions,
  those keep working.
- **Breaking ⌃R** or any history search the user already has (zsh's built-in, fzf, atuin).
- **Breaking prompt themes** such as powerlevel10k (including its instant prompt) or Starship.
- **Slowing down typing.** A keystroke that types must never wait on the core. A slow core means
  a missing suggestion, never a slow prompt. One stated exception: in bash, an explicit key press
  (accept, open the menu) may wait for the core, bounded at 50 ms, and does nothing on timeout
  (§3, §6).
- **Changing the user's history, or writing a copy of it anywhere.** The one file we write is the
  context file of §5.7: per command a key, the folder, the exit code and the time. Never the
  command text.
- **Suggesting a command the shell was told not to keep** (a leading space, `HISTIGNORE`, fish's
  private mode and the like; §5.8).
- **Fighting another plugin** that already does the same job. See §9.

## 3. Constraints

### Latency budget (AI's call)

These are targets for the builder to measure against, not measurements.

| Path | Target |
| --- | --- |
| Core answers a suggestion request | under 10 ms at p95, on a history of 100,000 lines |
| Core answers a menu request without generators | under 30 ms at p95 |
| A generator (e.g. git branches) | runs off the typing path; default timeout 500 ms; cached |
| Keystroke handling in the adapter | never waits for the core at all (async), except the bash explicit keys below |

The 10 ms figure is there so the ghost text keeps up with a fast typist. If a request misses its
budget the adapter shows nothing for that keystroke; it does not wait.

### bash explicit keys: a separate budget (AI's call)

Plain bash has no per-keystroke hook and no way to draw an answer later (§6), so an async answer
has nowhere to go. For bash only:

| Path | Target |
| --- | --- |
| Explicit key (accept a suggestion, open or filter the menu) | the adapter sends a `sync` request (§5.5) and waits **at most 50 ms**; on timeout it does nothing and the late answer is dropped |
| Core answers a `sync` suggestion request | under 10 ms at p95 (same as above) |
| Core answers a `sync` menu request | under 30 ms at p95; generators are never awaited, only cached generator values are included |
| Keys that only type | never wait; the bash adapter binds no typing keys |

The interactive bash menu blocks the line only while it is open: it is a modal loop the user
opened with a key and closes with Enter or Esc. Each filter keystroke inside it is an explicit
`sync` request with the same 50 ms bound.

### Never block the shell

- Adapters send a request and return to the line editor at once. The answer is drawn when it
  arrives (zsh: a file-descriptor handler, `zle -F`). The bash explicit keys above are the only
  bounded wait.
- Every request carries an id. An answer for an older buffer is dropped.
- If the core process is missing, crashed or hung, the adapter goes quiet for that session. It
  never prints errors at the prompt. A `doctor` command (§7) explains what happened.
- Adapters print nothing during shell startup. (powerlevel10k's instant prompt treats console
  output during initialization as a problem.)

### Async fetch

History loads once per core process, then stays in memory. It is updated by the adapter's "command
ran" reports, and re-read when the history file changes (§5.8). Generators run in background
threads or child processes with a timeout, and their results are cached by (generator, working
directory) for a short time. A menu shows static items first and adds generator items when they
arrive.

### Platforms and versions (AI's call)

- **macOS first.** Linux supported later (after the first release of each library). Windows is
  out (§2).
- **zsh: minimum 5.3,** for `add-zle-hook-widget`. `region_highlight` entries are tagged with
  `memo=` only on zsh 5.9+; on 5.8 and older the adapter removes only its own entries by
  remembering the exact strings it added and deleting entries equal to them (§6). The adapter
  checks the version with `is-at-least` and stays off, silently, below 5.3.
- **bash: plain readline needs 4.0+** (`bind -x` with `READLINE_LINE` and `READLINE_POINT`, and
  fractional `read -t` timeouts). **ble.sh runs on bash 3.0+,** so on macOS's system bash 3.2 the
  ble.sh path may work where plain readline can't (§6).
- **fish: minimum 3.0** (private mode). fish 4.0+ also honours `fish_should_add_to_history` (§5.8).
- The test matrix is in §14.

## 4. Decided (the user's decisions, as decided)

- **One repo, two libraries:** fuxx-autosuggestions and fuxx-autocomplete.
- **Our own core engine in Rust**, shared by both. It does history reading and ranking, command
  knowledge, and generators for live values. "We dont ever rely on zsh."
- **Our own command knowledge.** It does not borrow zsh's completion system. It starts from
  Fig's withfig/autocomplete specs (MIT), converted into our own format, with the original
  license notice kept. From then on the data is ours to maintain.
- **Thin adapters, no smarts:** zsh, bash and fish plugins, other terminal apps, and fuxx.
- **fuxx installs fuxx-completes as a plugin** (changed 2026-11-13; was "fuxx talks to the core
  directly, can draw a nicer menu, and switches each library on or off"). Inside fuxx the zsh
  adapter draws, as in any terminal; fuxx's Settings → Plugins has one On/Off switch for the whole
  plugin. fuxx drawing a nicer menu is later (§8).
- **Users:** people on zsh, bash or fish in most terminals (Ghostty, iTerm, Terminal.app and
  others), apps that embed the core, and fuxx. (Warp was listed here at first; the builder check
  found Warp replaces the shell's line editor, so it is a stay-off case: §9.)
- **Scope:** the plain shell prompt only, not inside AI agents.
- **Order:** autosuggestions first (small, mostly history) to prove the core and the adapters.
  Autocomplete after.
- **Repo policy:** one repo for this family. Libraries with a different job get their own `fuxx-…`
  repo under the same GitHub account. No GitHub org. The umbrella is the `fuxx-` name prefix, the
  GitHub topic `fuxx`, and a "Libraries" list in fuxx's README.
- **Timing:** building starts after fuxx's v0.5.0 release. The fuxx side (switches, loading the
  adapters, the nicer menu) becomes a milestone in fuxx's own roadmap later, not here.
- **Context data lives in a small local file (decided 2026-11-09).** The engine may keep a small
  file on the user's machine under `$XDG_DATA_HOME/fuxx-completes/` (default
  `~/.local/share/fuxx-completes/`). Per command it holds only the folder, the exit code and the
  time. It is never sent anywhere, honours the shell's "don't save this" rules, and is wiped with
  one command. With it, context ranking works across all shells and after restarts. The rule: no
  copy of the command history itself; only this small metadata file, keyed by commands already in
  the shell's own history. Details in §5.7 (the key is an AI's call).
- **License (AI's call):** MIT OR Apache-2.0, the Rust convention.
- **Layout (AI's call):** a Cargo workspace (§12).

## 5. Architecture

```
  zsh plugin     bash plugin     fish plugin     other apps          a host terminal
  (draws only)   (draws only)    (draws only)    (draw their own)    (draws its own UI)
       \              |              /               |                  |
        \             |             /                |      host channel: OSC on the pty
         +---- fuxx-completes binary ----------------+      (buffer, cursor, facts from
               (local protocol, §5.5)                        the shell adapter, §5.5)
                      |                                                 |
        +-------------+-------------------------------------------------+
        |        Rust crates (linked by the binary and by embedders; fuxx installs the
        |        zsh plugin and the binary as a fuxx plugin instead, §8)
        v
  fuxx-autosuggestions        fuxx-autocomplete
        \                        /
         +-- fuxx-completes-core +
             history · context file · ranking · line parsing · specs · generators · protocol types
```

### 5.1 The core (`core/`, crate `fuxx-completes-core`)

Holds every piece of logic that is not drawing:

- **History.** Reads the shell's history file read-only and keeps it in memory, re-reading it when
  it changes (§5.8). Supported formats: zsh (plain and `EXTENDED_HISTORY`,
  `: <start>:<elapsed>;<command>`, with zsh's metafied bytes decoded), bash (plain, and `#<epoch>`
  timestamp lines when `HISTTIMEFORMAT` is set), fish (`~/.local/share/fish/fish_history`). The
  adapter tells the core which file and format; reading a file format is parsing, not depending on
  a shell. The adapter also reports each command the shell saved, with its working directory and
  exit status, which the history file alone doesn't record.
- **Context file.** The small metadata file of §5.7: folder, exit code and time per command,
  keyed so that no command text is stored.
- **Shell state.** Names the adapter forwards at each prompt (aliases, functions, builtins, `PATH`,
  named dirs), kept in memory only (§5.9).
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

- **Built-in** generators written in Rust in the core, referenced by name (`git.branches`). Expect
  most useful generators to be these, git above all (§11).
- **Declarative** generators in a spec: a program and its arguments to run, plus how to split and
  trim its output.

Rules:

- Run the program directly with an argument list, never through `sh -c` or any shell. A generator
  that needs a pipe, a redirect or `$(…)` is written as a built-in Rust generator instead.
- Only generators from specs we ship (or the user's own spec folder) run. Nothing in the current
  directory or a cloned repo can add one.
- Timeout (default 500 ms, AI's call), then give up quietly.
- Cache per (generator, working directory) for a short time; the TTL is the builder's choice.
- Never on the keystroke path: the menu shows other items first.

### 5.4 Ranking

**Suggestions** start with parity with zsh-autosuggestions' default `history` strategy: the most
recent history entry that starts with the typed text. Then, in later milestones, context: same
working directory and whether it succeeded (from the context file, so across shells and restarts),
and the command that ran just before (from this session's records in memory; the context file has
no session field, §5.7). The weights are the builder's choice; ranking must be deterministic for a
given history and context, so it can be tested. A command missing from the history the core holds
is never suggested (§5.8).

**Menu items** are ordered by kind (subcommands, then options, then argument values; open to
change), then by prefix match quality, then by how often the user picked them (counted in memory
only; persisting them would need the user to widen §5.7 first).

### 5.5 The adapter protocol (AI's call, settled in the M0 plan)

The shell plugins can't link a Rust crate, so they talk to a small binary, `fuxx-completes`.

- One long-lived `fuxx-completes serve` child process per shell session, started by the adapter on
  the first prompt. It loads history once and keeps it warm. Starting a process on every keystroke
  would reload history each time and can't meet the budget.
- **Wire format 1 (AI's call, settled in the M0 plan).** One message per line, field-separated so
  shell code never parses JSON. `US` = byte `0x1F` separates fields, `LF` ends the message. Inside a
  field a newline is written as `RS` = `0x1E`; a field never otherwise contains `US` or `RS` (the
  adapter doesn't send such a buffer; the core answers "no suggestion" rather than send such text).

  | Direction | Message | Fields after the kind |
  | --- | --- | --- |
  | adapter → core | `hello` | protocol (`1`), shell (`zsh`), shell version, history format (`zsh`), history path (absolute, may be empty), libraries (comma list: `autosuggestions`) |
  | adapter → core | `suggest` | id (u64), mode (`async`/`sync`), cursor (code points), cwd, buffer |
  | adapter → core | `ran` | cwd (where it started), exit status (i32), time (Unix seconds), command |
  | adapter → core | `bye` | none |
  | core → adapter | `ready` | protocol (`1`) |
  | core → adapter | `suggestion` | id, text after the buffer (empty = none) |

  Unknown kinds are ignored. A `suggest` whose id parses but whose other fields don't still gets an
  empty `suggestion` for that id, so the adapter's one-in-flight slot always clears. A JSON encoding
  of the same messages is added when an embedder in another language needs one (deferred).
- **Request modes (AI's call).** Every request has a `mode`: `async` (the default; the adapter
  doesn't wait, the answer is drawn when it arrives) or `sync` (the bash explicit keys of §3; the
  adapter waits at most 50 ms). A `sync` request is answered ahead of queued `async` work and never
  waits for a generator. A late `sync` answer is dropped by id like any stale answer; the bash
  adapter reads and discards replies until it sees its own id or times out.
- Messages, roughly:
  - `hello` (shell, version, history file and format, which libraries are on) → `ready`
  - `suggest` (buffer, cursor, cwd, mode) → `suggestion` (text to show after the buffer, or none)
  - `complete` (buffer, cursor, cwd, mode) → `items` (label, insert text, kind, description),
    possibly followed by more `items` when generators finish (`async` only)
  - `ran` (command, cwd, exit status, time) → no reply. Sent only for commands the shell saved
    (§5.8).
  - `state` (aliases with expansions, function names, builtins, `PATH`, named dirs) → no reply
    (§5.9)
  - `bye`
- **Short-lived commands** besides `serve`:
  - `fuxx-completes report` (command, cwd, exit status, time, history file): appends one record to
    the context file and exits. Used by fish, which can't keep a `serve` process (§6). It loads no
    history: the key needs only the command text and the secret (§5.7).
  - `fuxx-completes complete …`: one menu answer for fish's `complete -a` (§6).
  - `fuxx-completes forget`: wipes the context file (§5.7).
  - `fuxx-completes doctor` (§7).
- Transport detail for zsh: zsh allows only one `coproc` at a time and other plugins may use it,
  so the adapter opens its own pair of **named pipes in a private temp folder** (**AI's call,
  M0**). The shell opens both pipes read-write first so no open blocks, starts the core on them
  (without the spare descriptors), opens its own write end of `req` and read end of `resp`
  (close-on-exec through `sysopen -o cloexec` on zsh 5.4+, so programs the user runs never
  inherit them; on 5.3.x the flag is silently lost, so there they do inherit them, harmlessly), and unlinks the pipes when the core's `ready` arrives, because earlier races the core's own open by path
  (**AI's call, M0**). A dead core gives EOF on the read end, and a write never
  raises `EPIPE` because the shell keeps a reader on `req`. **One request is in flight at a time**;
  a core that hasn't answered in 2 s is hung and is stopped, and so is one that answers `ready`
  with another protocol or when the shell exits. A core that is already gone (EOF) is never
  signalled, because its pid may belong to another process by now (**AI's call, M0**). A buffer over
  **1000 characters**, or one holding `0x1F` or `0x1E`, sends no request (**AI's call, M0**). The
  adapter finds the binary through `FUXX_COMPLETES_BIN`, then `PATH` (**AI's call, M0**).
  bash has the same single-coproc concern; named pipes opened with `exec` are the likely choice there, settled in M2.
- A shared per-user daemon on a Unix socket would save memory across many shells but adds
  lifecycle work (start, stop, stale sockets), and bash can't open a socket without an extra
  tool. Deferred until per-session processes prove too heavy.

The binary lives in `cli/` (package and binary `fuxx-completes`), depending on the core and the
libraries (**AI's call, settled in the M0 plan**).

Embedders: other Rust programs link the crates directly; programs in other languages run the
binary and speak wire format 1 (JSON deferred). fuxx runs the zsh adapter as a plugin (§8).

#### The host channel: OSC on the pty (AI's call)

A terminal app that wants to draw the ghost text or menu itself (fuxx, or any other host) still
needs the buffer and cursor, which only the shell has. The shell adapter forwards them over the
pty the shell already writes to.

- **Opt-in by the host.** The adapter uses this channel only when the host advertises it with
  `FUXX_COMPLETES_HOST=1` in the shell's environment. Without it, nothing is ever sent, so plain
  terminals never see these sequences. Inside tmux or screen (`TMUX` or `STY` set) the adapter
  doesn't use it, because a multiplexer swallows or misroutes unknown OSC sequences.
- **The sequence.** `ESC ] 7701 ; fuxx-completes ; 1 ; <message> ; <fields…> BEL`. **OSC 7701**
  was picked because it isn't in the published lists we know of (xterm's control sequences, and
  the iTerm2 1337, VS Code 633, FinalTerm 133, kitty and rxvt 777 families). It is namespaced by
  the mandatory first field `fuxx-completes` and a protocol version (`1`): a host ignores any OSC
  7701 whose first field isn't `fuxx-completes`, so a clash with another program that picks the
  same number is harmless. The builder re-checks the number against current terminal docs in M2.
- **Messages.** The same ones the adapter would send to `serve` (`hello`, `ran`, `state`), plus
  `buffer` (id, cursor as a code-point offset, cwd, the buffer) and `clear` (the line was run or
  abandoned). Free-text fields are base64, so a buffer holding `BEL`, `ESC` or `;` can't break the
  sequence. In host mode the adapter starts no `serve` process and draws nothing; the host feeds
  these messages to its own linked core and draws.
- **Accept.** When the user accepts in the host's UI, the host writes a key sequence to the pty
  as if typed: `ESC [ 7701 ~` (a CSI no keyboard sends), followed by base64 of (id, new buffer,
  new cursor) and a `BEL`. The adapter binds `ESC [ 7701 ~` to an "accept from host" widget
  (zsh) or `bind -x` function (bash) that reads the rest, bounded at 50 ms, and applies it only if
  the id matches the buffer it last forwarded. So the shell sets the line itself: the host never
  draws over the shell's own text. The exact read path inside ZLE and readline is verified in M2.
- **fish** can't forward the buffer as you type (no buffer-change hook, §6). In host mode it
  still reports commands, through `fuxx-completes report`, which the host's core sees in the
  shared context file.

### 5.6 The adapters

Adapters are small shell scripts in `adapters/<shell>/`, one plugin file per library so each can be
turned on alone (for example `fuxx-autosuggestions.plugin.zsh` and `fuxx-autocomplete.plugin.zsh`).
They:

- start the core process and send it requests;
- draw the answer with the shell's line editor;
- bind the accept keys, never rebinding a key the user (or another plugin) already bound (§6);
- report each command the shell saved, after the shell's own filtering (§5.8);
- send shell state at each prompt (§5.9);
- **forward the buffer and cursor to a host terminal** over the host channel of §5.5, when the
  host advertises `FUXX_COMPLETES_HOST=1`, and bind the "accept from host" key;
- detect conflicting plugins and terminals that replace the line editor, and stay off (§9).

In M0 the zsh adapter is one self-contained file, `adapters/zsh/fuxx-autosuggestions.plugin.zsh`,
that finds everything through `PATH` or variables, never through its own location.

They hold no ranking, parsing or command knowledge. Forwarding facts the shell already has (the
buffer, saved commands, alias expansions, `PATH`) is not relying on the shell: the core still
never calls one.

### 5.7 The context file (the user's decision; the key is an AI's call)

The user decided on 2026-11-09 that context data lives in a small local file. This section is how.

- **Where.** `$XDG_DATA_HOME/fuxx-completes/`, default `~/.local/share/fuxx-completes/`, on macOS
  too. The folder is mode `0700`, the files `0600`.
- **What a record holds.** A key, the folder the command ran in, its exit code, and the time (Unix
  seconds). Nothing else: no command text, no arguments, no environment, no session id.
- **The key (AI's call).** A keyed hash of the exact command text as the shell saved it (zsh's
  metafied bytes decoded, multi-line commands joined with `\n`): HMAC-SHA-256 or BLAKE3's keyed
  mode (the crate is checked through Context7 in M1), truncated to 128 bits. The secret is 32
  random bytes in a `key` file beside the records, created on first write. To use a record, the
  core hashes the commands it read from the shell's own history file and looks the keys up; a
  record whose command isn't in history can't be turned back into text and is never shown. Why
  keyed: a plain hash of short commands (`ls`, `git status`) could be reversed with a public
  table. Honest limit: someone who can read both files can still test guesses. The hash keeps the
  text out of the file; the file permissions protect the rest. The folder names are stored as
  they are, because ranking needs them.
- **One file per history file.** Records go to `context/<id>` where `<id>` is the keyed hash of
  the history file's path. A core only ever rewrites the file for its own history. It reads all
  of them, so a command run in bash ranks in zsh too, when the same text is in zsh's history.
- **Honours "don't save this".** Only commands the shell itself saved are reported (§5.8), so a
  command with a leading space, matched by `HISTIGNORE` or run in fish's private mode never
  produces a record.
- **Pruning.** When a core starts, and when a file passes its size cap, it compacts that file:
  records whose key matches no command in the current history file are deleted (so `history -d`,
  once the shell has written the file, leaves no trace), and the oldest records go first past the
  cap. The cap (a few MB) and the record encoding are the builder's choice. A command a shell has
  saved only in memory and not yet written may lose its records to a compaction by another
  session; that costs ranking, never privacy.
- **Concurrent writers.** Records are appended with `O_APPEND`, one small write each; compaction
  writes a new file under an advisory lock and renames it into place. Details in the M1 plan.
- **Wiped with one command.** `fuxx-completes forget` deletes the context files and the secret.
  Every running core sees the files are gone (it checks before using them) and drops its
  in-memory context too.
- **Never sent anywhere.** The core has no network code (§13). Tests point `XDG_DATA_HOME` at a
  temporary folder.

### 5.8 History semantics (AI's call)

The core suggests only commands the shell itself kept, after the shell's own filtering.

- **zsh:** a `zshaddhistory` hook (added with `add-zsh-hook`, so others keep theirs) notes the
  line; the adapter reports it only if zsh actually kept it, with the filter respected: the line is
  in zsh's history list and isn't one zsh drops later (`HIST_IGNORE_SPACE` with a leading space,
  `HIST_NO_STORE`, a `HISTORY_IGNORE` match, a line another `zshaddhistory` hook rejects, an alias
  with a leading space). **Reported one prompt later (the user's decision, 2026-11-13):** zsh keeps
  a line it won't save in its history list until the next command is entered, so the adapter holds
  each command back and reports it at the next prompt where another command went into history,
  once zsh has dropped such lines. The cost, accepted: the command just run isn't suggested until
  another runs, and a session's last command is never reported (memory only). The working
  directory is the one captured at `preexec`, the exit status and time those of the command
  itself. The check (verified on zsh 5.9, 2026-11-13): a line zsh won't keep lingers at its event
  number until the next command is entered, and that command then takes the same number. So at each
  prompt where a line was entered, the pending command (event `p`) is reported only if the newest
  event `n` is beyond `p` and `$history[p]` is still the same text; the new command then becomes
  pending unless it starts with a space under `HIST_IGNORE_SPACE`, is `history`/`fc -l` under
  `HIST_NO_STORE`, matches `HISTORY_IGNORE` (which only filters the file, so the adapter checks it
  itself) or is over 1000 characters. When in doubt it doesn't report.
- **bash:** a function added to `PROMPT_COMMAND` (appended, not replacing) runs `history 1` and
  reports the entry only if its history number is new since the last prompt. bash has already
  applied `HISTCONTROL` and `HISTIGNORE` by then. The exit status is captured first; the working
  directory is the one saved at the previous prompt, where the command started.
- **fish:** a `fish_postexec` handler reports the command unless it starts with a space, fish is
  in private mode (`fish --private`, `fish_private_mode` set), or, on fish 4.0+, a user's
  `fish_should_add_to_history` says no (when that function exists it decides alone, as in fish).
- **Re-reading the history file.** Before answering, the core checks the history file's size and
  modification time. If the file only grew, it reads the new part; otherwise it re-reads the
  whole file off the typing path and swaps the index in when done. So deletions such as
  `history -d` (once written) are respected, and commands other shells saved appear.
  A growth is trusted only on the same device and inode; anything else is a full re-read (AI's call, M0).
- **Session records** (from `ran`) fill the gap until the shell writes its file. They are reported
  only after the filter above, so a command the ignore rules hid is never suggested. They are ranked
  above file entries until the file holds them; each entry the core reads from the file retires one
  equal session record (AI's call, M0).

### 5.9 Shell state (AI's call)

Adapters may send the local core what the shell knows and the core can't read from a file:

- aliases (names and their expansions) and function names, never function bodies;
- builtins;
- `PATH`;
- named directories (zsh `hash -d`).

The adapter sends a `state` message at the first prompt and again at any prompt where it changed
(a plain string compare in shell, no parsing). The core uses it for command-position items, to
see through an alias to the command it runs (`g` → `git`), and to expand `~name`. It is kept in
memory only, never written to the context file, and never leaves the machine. This is
forwarding, so rule 5 holds. fish's short-lived calls inherit `PATH` from the environment; fish
sends no other state.

## 6. How each shell shows ghost text and the menu

Honest about what each shell allows.

### zsh (the first and best-supported adapter)

- **Versions:** 5.3 or later (§3).
- **Ghost text:** set ZLE's `POSTDISPLAY` to the part of the suggestion after the buffer, and add a
  `region_highlight` entry from `$#BUFFER` to `$#BUFFER + $#POSTDISPLAY` with a dim style (default
  `fg=8`, configurable). This is how zsh-autosuggestions does it. Other plugins such as
  zsh-syntax-highlighting also write `region_highlight`, so we only ever remove our own entries:
  on zsh 5.9+ we tag them with `memo=`; on 5.8 and older we remember the exact strings we added
  and remove only entries equal to them. (Honest limit: if another plugin added a byte-identical
  entry, the fallback could remove that one too.)
- **Widget wrap:** like zsh-autosuggestions, wrap the widgets that edit the buffer so that after
  each edit we request a new suggestion, and wrap the accept widgets. Wrapping happens at the
  first `precmd`, so widgets defined by plugins loaded after us are seen.
- **Keys:** at the end of the buffer, `forward-char` and `end-of-line` accept the whole suggestion
  and `forward-word` accepts up to where the cursor moves. In stock zsh's emacs keymap that means
  → and `^E` for all of it and `Esc f` for a word; End and ⌥→ have no binding by default. So the
  adapter binds End (`^[[F`, `^[[4~`) to `end-of-line` and ⌥→ (`^[[1;3C`) to `forward-word`
  **only if those sequences are unbound**, and never rebinds a user's binding. The bindings are made in the `emacs` and `viins` keymaps; `vicmd` is left alone (AI's call, M0). Terminals differ in
  what ⌥→ sends; `doctor` shows what is bound.
- **vi mode is in scope.** As zsh-autosuggestions does, the accept widgets include
  `vi-forward-char`, `vi-end-of-line` and `vi-add-eol`, and the partial-accept widgets include
  `vi-forward-word`, `vi-forward-word-end`, `vi-forward-blank-word`, `vi-forward-blank-word-end`,
  `vi-find-next-char` and `vi-find-next-char-skip`, alongside `forward-word` and
  `emacs-forward-word`.
- **Async:** the answer arrives on a file descriptor watched with `zle -F`; the handler sets
  `POSTDISPLAY` and redraws (`zle -R`).
- **Hooks:** use `add-zle-hook-widget` and `add-zsh-hook` instead of overwriting `zle-line-init`,
  `precmd`, `preexec` or `zshaddhistory`, so powerlevel10k and others keep theirs. Command reports
  follow §5.8.
- **Menu:** zsh has no popup. The menu is drawn below the line, either as extra lines in
  `POSTDISPLAY` styled with `region_highlight`, or with `zle -M`. We don't use `compadd` or the
  completion system to draw it, because that would tie the menu to zsh's completion machinery.
  Keys to move through and pick items are bound in a keymap of our own while the menu is open.
  Which drawing method holds up best (long lists, narrow terminals, multi-line prompts) is
  settled in M6.
- **Tab:** we don't take Tab by default. zsh's own completion stays on Tab.
- **Host mode:** with `FUXX_COMPLETES_HOST=1`, the same wrapped widgets forward the buffer over the
  host channel instead of drawing (§5.5).
- **Limits:** a menu below the line pushes the screen and can't overlap other text; very long
  buffers and multi-line prompts need testing.

### bash

- **Versions:** plain readline needs bash 4.0+; ble.sh runs on bash 3.0+ (§3). macOS ships bash
  3.2: there the plain-readline adapter says so in `doctor` and stays off, and the ble.sh path is
  the hope.
- **Plain readline has no ghost text hook.** There is no `POSTDISPLAY` equivalent and no
  per-keystroke hook. Drawing grey text with escape codes behind readline's back breaks on redraw,
  so we won't.
- **What plain bash can do:** `bind -x` runs a shell function on a key and lets it read and set
  `READLINE_LINE` and `READLINE_POINT`. So a key can insert the current best suggestion, and a key
  can open a menu (printed below, then the prompt redrawn). Not as-you-type.
- **Bounded wait (AI's call):** those explicit keys send a `sync` request and wait at most 50 ms
  (`read -t 0.05` on the core's output); on timeout they do nothing. The menu, once open, is a
  modal loop that blocks only while it is open. Keys that only type are never bound. This is the
  one stated exception to "never wait" (§3, AGENTS rule 6).
- **ble.sh** replaces readline with a line editor written in bash and has its own grey
  auto-complete. When ble.sh is loaded, whether we can feed our suggestions into it through its
  own interfaces is an open question for the bash milestone. If we can't, ble.sh users keep its
  built-in feature and we stay off.
- **History:** `PROMPT_COMMAND` with `history 1` (§5.8).
- **Host mode:** bash can forward the buffer only at the explicit keys (there is no buffer-change
  hook in plain readline), and binds the "accept from host" key (§5.5).
- **Tab:** bash's `complete` is left alone by default.

### fish

- **Versions:** 3.0 or later (§3).
- **fish already has autosuggestions,** from history and its completions, on by default, and
  offers no public hook to supply our own ghost text. So fuxx-autosuggestions **stays off in
  fish**: fish's own version is good and two ghost texts can't share one line.
- **Reporting commands (AI's call): short-lived `fuxx-completes report` calls.** fish has no
  coproc and can't keep an extra file descriptor open across prompts, so it can't hold a `serve`
  process. From `fish_postexec` (with the filter of §5.8) the adapter starts
  `fuxx-completes report` in the background and disowns it, so the prompt never waits and fish
  prints no job message (checked in M2). We report rather than not at all because the context file
  makes a report cheap (one append, no history load) and gives the user's other shells, and a host
  such as fuxx, the context of commands run in fish.
- **Menu:** fish's `complete -c <command> -a '(…)'` can call our binary for argument values, and
  fish shows them in its own pager on Tab. That uses fish's completion display, not as-you-type.
  **fish has no priority between sources:** every `complete -c` definition for a command is
  merged, and fish autoloads its own. **AI's call:** we register only for commands fish has no
  completions for (no `complete -c` definition and no `<command>.fish` in any
  `$fish_complete_path` folder), checked at the first prompt over our short spec list. A user who
  forces ours on for a command fish already covers gets both lists merged; fish can't rank one
  above the other.
- **Host mode:** fish has no buffer-change hook, so it can't forward the buffer as you type.
  Commands still reach a host through `report` and the shared context file.
- **Limits:** no as-you-type menu in fish, no ghost text of ours. Stated plainly in the README.

### Other terminal apps and fuxx

They get structured answers (JSON, or Rust types) and draw however they like: a popup at the
cursor, a ghost-text overlay. They need the buffer and cursor, which only the shell has, so they
pair with a shell adapter over the host channel of §5.5 (or with shell integration of their own).

## 7. Configuration and the doctor

Small and later. Each adapter reads a few shell variables (on/off per library, highlight style,
accept keys). `fuxx-completes doctor` prints what is loaded, which conflicting plugins or
terminals were found, the shell and version, which accept keys were bound or left alone, whether
host mode is on, where the context file is and how big, and the core's last error.
`fuxx-completes forget` wipes the context file (§5.7). No config DSL. Details belong in the M1
plan.

## 8. How fuxx uses it (changed 2026-11-13)

fuxx installs fuxx-completes as a **fuxx plugin** (fuxx's milestones M11y–M11z, its note 76). It
no longer links the crates.

- **What this repo ships for it:** a `fuxx-plugin.conf` at the top of the repo, and on each release
  the assets `fuxx-plugin-macos-arm64.zip` and `fuxx-plugin-macos-x86_64.zip`, each a folder with
  that file, the `fuxx-completes` binary in `bin/` and the zsh adapter in `shell/`:
  ```
  format      = 1
  id          = fuxx-completes
  name        = fuxx-completes
  version     = <release version>
  description = Suggestions from your history as you type.
  bin         = bin
  zsh         = shell/fuxx-autosuggestions.plugin.zsh
  ```
  The zsh file follows §5.6's one-file-per-library naming (the user's decision, 2026-11-13).
- **What fuxx does:** puts `bin/` on `PATH` and sources the zsh adapter in its zsh terminals after
  the user's `.zshrc`, with `FUXX_PLUGIN_DIR` and `FUXX_PLUGIN_DATA` set while it is sourced. The
  adapter starts `serve` and draws ghost text as it does in any terminal. fuxx's On/Off switch
  covers the whole plugin; finer settings stay the adapter's shell variables (§7).
- **Data:** when `FUXX_PLUGIN_DATA` is set, the context file (§5.7) lives there instead of
  `$XDG_DATA_HOME/fuxx-completes/`, so uninstalling the plugin from fuxx removes it. A copy installed
  for other terminals keeps its own data. The adapter keeps its own copy of the variable (fuxx sets
  it only while sourcing).
- **Double-load guard (AI's call, 2026-11-13):** the adapter sets a marker when it loads; a second
  copy in the same shell (fuxx's plugin plus a line in the user's `.zshrc`) sees it and stays off,
  shown by `doctor`. One copy runs.
- **Later, not now:** fuxx drawing a nicer menu itself through the host channel (§5.5,
  `FUXX_COMPLETES_HOST=1`). It needs M2 and M6 here and a drawing API in fuxx's plugin format that
  format 1 doesn't have. This repo still ships the adapter side of the host channel (M2) and keeps
  the crate APIs usable for any embedder.

## 9. Coexisting with other plugins (AI's call)

Rule: **detect it, and stay off.** Never fight another tool for the same keys or the same space on
the line.

| Found | What we do |
| --- | --- |
| zsh-autosuggestions (e.g. `_zsh_autosuggest_start` defined) | fuxx-autosuggestions stays off in that shell |
| zsh-autocomplete (functions named `.autocomplete__*` in current versions, `.autocomplete.*` in older ones) | fuxx-autocomplete stays off |
| fzf key bindings (`fzf-history-widget`, `fzf-file-widget`, `fzf-completion`) | we never bind ⌃R, ⌃T or Tab; if `fzf-completion` holds Tab, our menu never uses Tab |
| atuin | we never bind ⌃R or ↑ |
| fish's own autosuggestions | fuxx-autosuggestions off in fish (§6) |
| ble.sh auto-complete | stay off unless the bash milestone finds a clean way in |
| **Warp** (`TERM_PROGRAM=WarpTerminal`) | both libraries stay off in every shell. Warp's own input editor replaces ZLE (and readline), and Warp lists ZLE plugins such as zsh-autosuggestions as unsupported |
| Any key sequence already bound by the user or a plugin | we leave it bound as it was (§6) |
| **Another copy of fuxx-completes** already loaded in this shell (fuxx's plugin and the user's rc file both load it) | the second copy stays off (§8's double-load guard) |

Detection runs at the first `precmd` (zsh) or first prompt, after the user's rc file has loaded
everything. Staying off is silent at the prompt and shown by `doctor`. A user can force ours on
(not in Warp, where our widgets would never run).

## 10. Prior art

- **zsh-autosuggestions:** fish-like ghost text in zsh via `POSTDISPLAY`, `region_highlight` and
  widget wrapping; strategies `history`, `completion` and `match_prev_cmd`; an async mode. Our zsh
  adapter's drawing, accept widgets and vi-mode handling follow its proven approach.
- **zsh-autocomplete:** a real-time completion listing in zsh, built on zsh's completion system
  (`compinit`, compsys). Shows how good as-you-type can feel; we differ by not using compsys.
- **Fig / Amazon Q Developer CLI:** a popup menu at the cursor in many terminals, driven by
  open-source TypeScript specs (withfig/autocomplete, MIT). Amazon announced the acquisition of
  Fig in August 2023. Fig's features became CodeWhisperer for command line, then Amazon Q
  Developer CLI in 2024, and the Fig app shut down on 1 September 2024. The Q Developer CLI client
  was later open-sourced as aws/amazon-q-developer-cli. Fig's specs are our starting data.
- **inshellisense (Microsoft):** IDE-style, as-you-type autocomplete for many shells (bash, zsh,
  fish, PowerShell and others), MIT-licensed, built on Fig's withfig/autocomplete specs. The
  closest prior art. It is a Node.js program: you start an inshellisense session on your shell
  (`is`), and it draws a box of suggestions itself. How we differ, honestly: a Rust core with no
  runtime to install; thin native adapters that work inside each shell's own line editor (ZLE,
  readline, ble.sh) rather than a session of our own around the shell; ghost text from history as
  a first-class library, with context from the folder and exit code; and a host channel so a
  terminal such as fuxx draws the UI itself. It is more mature and covers more shells today.
- **atuin:** records your commands into its own SQLite database alongside the shell's history
  file, which it does not replace; optional encrypted sync, and a search UI on ⌃R. A different
  job; we coexist with it.
- **carapace:** a multi-shell completion engine in Go with its own spec format and bridges to
  other completers. Close in spirit to our core; worth studying before the autocomplete plan.
- **fish's built-ins:** autosuggestions from history and completions, and completions generated
  from man pages. The bar for how it should feel.

## 11. Importing Fig specs

- **Source:** github.com/withfig/autocomplete, `src/`, one TypeScript file per command, each
  exporting a `Fig.Spec`: `name`, `description`, `subcommands`, `options` (with `name`, `args`,
  `isRepeatable`, `exclusiveOn` and more), `args` (with `isOptional`, `isVariadic`, `template`
  such as `"filepaths"`/`"folders"`, `suggestions`, `generators`). `pnpm build` compiles them to
  JavaScript in `build/`.
- **Conversion:** a one-off converter (a dev tool run by maintainers, not something users install)
  loads the compiled specs and writes our format. Declarative parts (names, descriptions,
  subcommands, options, fixed suggestions, templates) convert directly.
- **Generators mostly don't convert.** Many Fig `script` values are shell strings that use pipes,
  redirects and `$(…)`, and others are functions; `postProcess` and `custom` are JavaScript. Under
  the "never through `sh -c`" rule (§5.3), only a generator whose script is one plain program with
  fixed arguments, and whose output needs only simple splitting, converts to our declarative form.
  Expect far fewer of those than the spec count suggests. The rest are dropped, or rewritten by
  hand as built-in Rust generators for the commands that matter most. **git first, and git's
  generators will be hand-written in Rust.**
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
cli/                       the fuxx-completes binary (serve; later report, complete, forget, doctor)
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
- History files are read, never written. No copy of the history is saved anywhere.
- The only file we write is the context file (§5.7): per command a keyed hash, the folder, the
  exit code and the time. It honours the shell's "don't save this" rules, is never sent anywhere,
  and `fuxx-completes forget` wipes it.
- Session records and shell state live in memory only.
- Generators run local programs only.
- Logs never contain command lines by default. A debug log, if any, is local, opt-in and says so.
- fuxx passes the buffer to the crates in-process; the host channel carries it only over the
  local pty between the shell and the host. It doesn't go anywhere else.

## 14. Testing strategy

- **Core unit tests:** history parsing for each format (including zsh metafied bytes, multi-line
  commands, bash timestamps, fish's file), ranking with fixed histories, line parsing edge cases,
  spec loading.
- **Context file tests:** the file never contains a command's text (search it for every command
  written); records for commands missing from history are pruned; `forget` removes everything and
  running cores drop their context; modes `0700`/`0600`; concurrent appends from two cores.
- **History semantics tests:** a leading space, `HISTIGNORE`/`HISTCONTROL`, zsh
  `HISTORY_IGNORE`, fish private mode and `fish_should_add_to_history` never produce a suggestion
  or a record; `history -d` then a write removes the command from suggestions.
- **Golden tests:** Fig conversion input → expected output for a few specs.
- **Protocol tests:** start the binary, send requests, check responses and ids, check stale answers
  are dropped, a hung generator times out, and a `sync` request is answered within budget or
  dropped.
- **Host channel tests:** a fake host reads the pty of a test shell started with
  `FUXX_COMPLETES_HOST=1`, decodes the OSC 7701 messages, sends the accept sequence and checks the
  buffer; with the variable unset, no sequence ever appears.
- **Latency benchmarks:** suggestion and menu requests against a synthetic 100,000-line history;
  fail when the budget in §3 is exceeded.
- **Adapter tests:** drive a real zsh (then bash, fish) in a pseudo-terminal with `zsh -f` or
  equivalent, a **temporary HOME**, `ZDOTDIR` and `XDG_DATA_HOME`, and a generated history file.
  Never the user's real rc files, history or context file.
- **Coexistence tests:** load zsh-autosuggestions, zsh-autocomplete, fzf's bindings and
  zsh-syntax-highlighting in the temp shell; check we stay off or don't break them; check Tab and
  ⌃R still do what they did; check a pre-bound End or ⌥→ is left alone; check
  `TERM_PROGRAM=WarpTerminal` keeps us off.
- **Test matrix (AI's call).** On macOS (current and previous release) first, Linux (current
  Ubuntu LTS) once Linux is supported:

  | Shell | Versions |
  | --- | --- |
  | zsh | 5.3 (minimum), 5.8 (last without `memo=`), latest 5.9.x; emacs and vi keymaps |
  | bash, plain readline | 4.0 (minimum), latest 5.x |
  | bash with ble.sh | 3.2 (macOS's system bash), latest 5.x |
  | fish | 3.0 (minimum), latest 3.x, latest 4.x |

  Old zsh versions come from source builds (`scripts/build-zsh.sh`, cached in `target/zsh/`); the
  matrix runs with `scripts/test-zsh-matrix.sh` (AI's call, M0). bash and fish versions are picked
  in M2.
- **Manual checks** (can't be automated well; say so): ghost text and menu look right in Ghostty,
  iTerm, Terminal.app and fuxx, with powerlevel10k's instant prompt, in a narrow window, and stay
  off in Warp.

## 15. Milestones

| | Milestone | Done when |
| --- | --- | --- |
| M0 | Core + history + zsh adapter | the binary serves suggestions from a zsh history file; the zsh plugin shows ghost text on zsh 5.3+ (`memo=` on 5.9+, exact-string fallback below); `forward-char`/`end-of-line` accept, `forward-word` accepts a word, vi-mode widgets wrapped; End and ⌥→ bound only if unbound; reports follow §5.8 and the history file is re-read on change; async; temp-HOME tests pass |
| M1 | Autosuggestions: context file and coexistence | the context file of §5.7 (keyed records, pruning, `forget`); ranking uses cwd and exit status across shells and restarts, and the previous command in-session; conflicting plugins and Warp detected; `doctor`; latency benchmark meets §3; the context file under `FUXX_PLUGIN_DATA` when set, and the double-load guard (§8) |
| M2 | bash and fish adapters, and the host channel (autosuggestions) | bash 4.0+: explicit keys with `sync` requests bounded at 50 ms, history via `PROMPT_COMMAND`; ble.sh path decided (bash 3.0+); fish: stays off cleanly, reports commands through `fuxx-completes report`; host channel: zsh and bash forward buffer, cursor and facts over OSC 7701 when `FUXX_COMPLETES_HOST=1`, the "accept from host" key is bound, the number is re-checked, and host channel tests pass with a fake host |
| M3 | Release fuxx-autosuggestions | Homebrew tap, zsh plugin manager install, crates.io; README install section made real; `fuxx-plugin.conf` and the `fuxx-plugin-macos-*.zip` release assets (§8), installed and checked in fuxx |
| M4 | Spec format + Fig import | format settled; converter; git and a few common tools imported (declarative parts; convertible generators only); notices in place |
| M5 | Line parsing, generators and shell state | current command and word found; built-in Rust generators (git first) and declarative generators with timeout and cache; adapters send `state` (§5.9) and the core uses it |
| M6 | zsh menu | as-you-type menu in zsh drawn below the line; Tab untouched; fzf-safe |
| M7 | bash and fish menus | key-driven bash menu (blocks only while open, `sync` bounded at 50 ms); fish argument values through `complete`, only for commands fish lacks |
| M8 | Release fuxx-autocomplete | packaged like M3 |

The fuxx side is fuxx's plugin system (M11y–M11z in fuxx's roadmap). fuxx drawing the nicer menu
over the host channel is later, in both repos.

### Left to the builder

- The transport and encoding details of §5.5, the binary's home, and crate dependencies (each
  checked through Context7 first), including the keyed-hash crate for §5.7.
- The context file's record encoding, size cap and lock (§5.7).
- Ranking weights, cache TTLs, the spec file format (JSON or TOML).
- How the zsh menu is drawn (§6), and the ble.sh question.
- The exact read path for the "accept from host" key in ZLE and readline (§5.5).
- How old shell versions are obtained for the test matrix (§14).
- Packaging details for each install channel.

## 16. Risks (recorded honestly)

- **Fig needed a funded team and still got acquired.** A full product of this kind is a lot of
  work, and it didn't survive alone.
- **inshellisense already exists** and covers more shells. Our reason to exist is the native,
  in-editor approach, ghost text with context, and host embedding (§10); if those don't hold up,
  say so.
- **Command knowledge needs ongoing upkeep.** CLIs change every release; imported specs go stale,
  and most generators must be hand-written (§11).
- **Terminals that replace the line editor** (Warp today) can't run any shell plugin. We detect
  and stay off; we can't fix it.
- **The OSC number is our own pick.** A clash is made harmless by the namespace field, but a host
  or multiplexer may still mishandle unknown sequences; hence opt-in only (§5.5).
- **`setopt shglob` and zsh's own hook helper.** Under `shglob`, zsh 5.9's `add-zle-hook-widget`
  dispatcher prints "No such widget `1:…'" at every prompt for any plugin that uses it. We keep
  the hook helpers (AGENTS.md says to use them) and revisit this in M1's coexistence work.
- **zsh 5.3.x leaks the adapter's pipe fds.** Close-on-exec works from zsh 5.4, so on 5.3.x the
  adapter's pipe descriptors are inherited by programs the user runs (harmless, but there). §5.5.
- **One maintainer with AI help.** Time is the limit.
- **Start small.** Autosuggestions in zsh first; a short spec list; no feature before it's needed.
- Shell limits are real: bash and fish will never match zsh here (§6).

## 17. Open questions

1. ~~Line-delimited JSON for everyone, or a simpler encoding for shell adapters (§5.5)?~~ Answered
   (M0 plan): field-separated for shells (wire format 1); JSON deferred until an embedder needs it.
2. Can ble.sh take our suggestions through its own interfaces (§6)?
3. JSON or TOML for specs, compiled in or loaded from disk (§5.2)?
4. Should menu pick counts persist across sessions? The context file holds only folder, exit code
   and time (§5.7), so persisting them needs the user to widen that decision first (§5.4).
5. Which commands make the first spec list beyond git (§11)?
