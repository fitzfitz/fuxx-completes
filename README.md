# fuxx-completes

Ghost-text suggestions and an as-you-type completion menu for zsh, bash and fish, from one Rust
engine that doesn't rely on any shell's completion system.

> **Status: pre-alpha, design stage. Nothing to install yet.** There is no working code in this
> repo, only the design ([`docs/design.md`](docs/design.md)), the roadmap and an empty Cargo
> workspace. Building starts after [fuxx](https://github.com/fitzfitz/fuxx-cli)'s v0.5.0 release.

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
              (local protocol; proposal: line-delimited JSON)           |
                      |                                                 |
        +-------------+-------------------------------------------------+
        v                    Rust crates (linked directly by fuxx)
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
- The one file we keep is small: `~/.local/share/fuxx-completes/` (or under `$XDG_DATA_HOME`).
  Per command it holds only the folder, the exit code and the time, under a keyed hash instead of
  the command's text. That's what lets suggestions use the folder you're in, across shells and
  restarts.
- Commands your shell was told not to keep (a leading space, `HISTIGNORE`, fish's private mode)
  never get a record and are never suggested.
- Nothing is ever sent anywhere. `fuxx-completes forget` wipes the file.

(Planned, like everything here.)

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
links the crates directly, draws its own nicer menu, and gets one switch per library in
Preferences. The shell plugin passes it the line you're typing over the terminal (an OSC sequence,
only when the terminal asks for it with `FUXX_COMPLETES_HOST=1`), and any terminal app can do the
same. fuxx's side is planned in fuxx's own roadmap.

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
