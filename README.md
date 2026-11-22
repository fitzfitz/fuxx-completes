# fuxx-completes

Ghost-text suggestions and an as-you-type completion menu for zsh, bash and fish, from one Rust
engine that doesn't rely on any shell's completion system.

> **Status: pre-alpha: the zsh ghost text is built from source (M0) and waiting for a check in a
> real terminal; nothing to install yet.** The design is in [`docs/design.md`](docs/design.md), the
> progress in [`docs/roadmap.md`](docs/roadmap.md). The menu and bash and fish are not built.

## The two libraries

### fuxx-autosuggestions

Grey ghost text that completes the command you're typing from commands you ran before.
**→** or **⌃E** accepts all of it; **Esc f** accepts the next word. **End** and **⌥→** do the same
only if you haven't bound them to something else: we bind them only when they're free, and never
take a binding of yours. vi mode works too.

```
~/proj $ git push o█rigin feature/login
                    └──────┬──────────┘
                     grey: from your history
```

Which command it offers: one you ran successfully in this folder comes first, the more often the
better (except `cd`, `pushd` and `open`, which rank only by where they ran); then the one that
usually follows the command you just ran; then the newest. Commands that only ever failed come
last; stopping one with ⌃C doesn't count as failing.

If zsh-autosuggestions is loaded too, ours stays off (`FUXX_AUTOSUGGESTIONS_FORCE=1` turns it on
anyway). In Warp it always stays off: Warp replaces the shell's line editor. Two copies of the
plugin in one shell (fuxx's and one in your `.zshrc`): the first one runs.

When you pause (3 seconds; an empty line counts too), a numbered list of up to 5 suggestions
appears under the line:

```
~/proj $ npm r█un dev
  1  npm run dev
  2  npm run build
  3  npm run test
```

**⌥1–⌥5** put that one into the line (on a US keyboard; elsewhere use the arrows), **↓/↑** move
through the list and **Enter** or **Tab** take the highlighted one; nothing is ever run for you.
**Esc** closes it, and so does typing. With nothing highlighted, ↑, Enter and Tab do what they
always did. Settings: `FUXX_AUTOSUGGESTIONS_LIST=off`, `…_LIST_EMPTY=off` (not on an empty line),
`…_LIST_DELAY=1.5` (seconds), `…_LIST_SIZE=3` (1–5).

Right after a command fails, the empty line offers a fix in grey: `npm run dev` in a pnpm project
becomes `pnpm run dev`, `cagro test` becomes `cargo test`, `git psuh` becomes `git push`. → takes
it; nothing is run for you. A command that worked before and failed now gets no "fix".

Some lines are never offered: anything that looks like it holds a secret (a token, a password in a
URL or a `PASSWORD=` value), and dangerous commands (`rm -rf`, `git push --force`,
`git reset --hard`, `kubectl delete`, …) until you've typed their dangerous part yourself
(`rm -`, `git push -`). A habit in a repo carries to its subfolders and its other worktrees.

`fuxx-completes doctor` shows what is loaded, which keys were bound or left alone, why it is off
if it is, and where the data is.

### fuxx-autocomplete

A menu while you type: commands, subcommands, flags, arguments, git branches, files.

```
~/proj $ git checkout fe█
          ┌──────────────────────────────┐
          │ feature/login       branch   │
          │ feature/search      branch   │
          │ fetch-fix           branch   │
          └──────────────────────────────┘
```

(Illustrations of the plan, not screenshots.)

## Planned install

*Planned, not available.* Nothing below works yet.

- **Homebrew:** a tap with the `fuxx-completes` binary and the shell plugins.
- **zsh plugin managers:** the plugin files in `adapters/zsh/`, loadable by the usual managers or by
  `source`.
- **crates.io:** `fuxx-completes-core`, `fuxx-autosuggestions` and `fuxx-autocomplete`, for apps
  that embed them.

## Shell support (planned)

| | Ghost text | Menu |
| --- | --- | --- |
| **zsh** | Yes, as you type (first target) | Yes, as you type, below the line; Tab stays zsh's |
| **bash** | Plain readline (bash 4.0+): key-driven only, it can't show ghost text. ble.sh (bash 3.0+): open question | Key-driven |
| **fish** | No: fish has its own, and ours stays off | On Tab, in fish's own pager, only for commands fish has no completions for |

Versions: zsh 5.3+, bash 4.0+ (or 3.0+ with ble.sh), fish 3.0+. macOS first, Linux later, no
Windows.

Works in most terminals: Ghostty, iTerm, Terminal.app and others. **Not in Warp:** Warp's own input
editor replaces the shell's, so shell plugins like this can't run there, and both libraries stay
off. Not used inside AI agents such as Claude Code, which have their own suggestions. If you
already use zsh-autosuggestions or zsh-autocomplete, the matching fuxx library stays off. It never
takes ⌃R, ⌃T or Tab from you.

In bash, the accept and menu keys may wait up to 50 ms for an answer (and do nothing if it's
late); typing itself never waits, in any shell.

## Architecture

```
  zsh plugin     bash plugin     fish plugin     other apps          fuxx
  (draws only)   (draws only)    (draws only)    (draw their own)    (draws a nicer menu)
       \              |              /               |                  |
        +---- fuxx-completes binary ----------------+                  |
              (local protocol, wire format 1)                                |
                      |                                                 |
        +-------------+-------------------------------------------------+
        v      Rust crates (linked by the binary and by embedders)
  fuxx-autosuggestions        fuxx-autocomplete
        \                        /
         +-- fuxx-completes-core +
             history · ranking · line parsing · specs · generators
```

All the logic lives in the core. The shell plugins only draw and forward keys and facts. The core
never calls a shell and has no network code: your history and command lines never leave your
machine.

## Your data

- Your history file is read, never written, and never copied.
- The one file we keep is small: `~/.local/share/fuxx-completes/` (or under `$XDG_DATA_HOME`;
  inside fuxx, in fuxx's folder for the plugin, so uninstalling removes it). Per command it holds
  only the folder, the exit code and the time, under a keyed hash instead of the command's text,
  and the key is a random secret beside it. That's what lets suggestions use the folder you're
  in, across shells and restarts. It stays under 4 MiB, and records of commands no longer in your
  history are pruned.
- Commands your shell was told not to keep (a leading space, `HISTIGNORE`, fish's private mode)
  never get a record and are never suggested.
- Nothing is ever sent anywhere. `fuxx-completes forget` wipes the file and the secret; shells
  that are running notice and start over.

## Repo layout

```
Cargo.toml                 Cargo workspace
core/                      fuxx-completes-core: the shared engine
fuxx-autosuggestions/      the ghost-text library
fuxx-autocomplete/         the menu library
specs/                     command knowledge (our own format)
adapters/zsh|bash|fish/    thin shell plugins
docs/design.md             the design (source of truth)
docs/roadmap.md            milestones and progress
THIRD_PARTY_NOTICES.md     licenses of imported data
```

## How fuxx uses it

[fuxx](https://github.com/fitzfitz/fuxx-cli), a GPU terminal app for running AI coding agents,
installs fuxx-completes as a fuxx plugin, with one On/Off switch for the whole plugin
([`docs/design.md`](docs/design.md) §8). It no longer links the crates. Later, the shell plugin can
pass fuxx the line you're typing over the terminal (an OSC sequence, only when the terminal asks
for it with `FUXX_COMPLETES_HOST=1`) so fuxx can draw a nicer menu, and any terminal app can do the
same. fuxx's side is tracked in fuxx's own roadmap.

## Credits and prior art

- [zsh-autosuggestions](https://github.com/zsh-users/zsh-autosuggestions): the ghost-text approach
  our zsh plugin follows.
- [zsh-autocomplete](https://github.com/marlonrichert/zsh-autocomplete): real-time completion in zsh.
- [Fig / withfig/autocomplete](https://github.com/withfig/autocomplete): our command knowledge
  starts from its MIT-licensed specs. See [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md). (Amazon
  announced its acquisition of Fig in August 2023; Fig became CodeWhisperer for command line, then
  Amazon Q Developer CLI in 2024, and the Fig app shut down on 1 September 2024. The Q Developer
  CLI client was later open-sourced as
  [aws/amazon-q-developer-cli](https://github.com/aws/amazon-q-developer-cli).)
- [inshellisense](https://github.com/microsoft/inshellisense) (Microsoft): multi-shell,
  as-you-type autocomplete on Fig's specs, MIT. The closest prior art. We differ with a Rust core,
  thin plugins inside each shell's own line editor, ghost text from history, and a channel for
  terminal apps to draw the UI. See [`docs/design.md`](docs/design.md) §10.
- [carapace](https://carapace.sh): a multi-shell completion engine.
- [atuin](https://github.com/atuinsh/atuin) and [fish](https://fishshell.com)'s built-in
  autosuggestions.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT)
at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

## Contributing

It's early. Read [`docs/design.md`](docs/design.md) first, and open an issue before a pull request
so we can check it fits the plan. Tests must never touch your real shell rc files or history.
