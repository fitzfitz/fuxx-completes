# fuxx-completes

Ghost-text suggestions and an as-you-type completion menu for zsh, bash and fish, from one Rust
engine that doesn't rely on any shell's completion system.

> **Status: pre-alpha, design stage. Nothing to install yet.** There is no working code in this
> repo, only the design ([`docs/design.md`](docs/design.md)), the roadmap and an empty Cargo
> workspace. Building starts after [fuxx](https://github.com/fitzfitz/fuxx-cli)'s v0.5.0 release.

## The two libraries

### fuxx-autosuggestions

Grey ghost text that completes the command you're typing from commands you ran before.
**→** accepts all of it; **⌥→** accepts the next word.

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
| **bash 4+** | Key-driven only; plain readline can't show ghost text. ble.sh: open question | Key-driven |
| **fish** | No: fish has its own, and ours stays off | On Tab, in fish's own pager |

Works in any terminal: Ghostty, iTerm, Warp, Terminal.app and others. Not used inside AI agents
such as Claude Code, which have their own suggestions. If you already use zsh-autosuggestions or
zsh-autocomplete, the matching fuxx library stays off. It never takes ⌃R, ⌃T or Tab from you.

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

All the logic lives in the core. The shell plugins only draw and forward keys. The core never calls
a shell and has no network code: your history and command lines never leave your machine.

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
Preferences. That work is planned in fuxx's own roadmap.

## Credits and prior art

- [zsh-autosuggestions](https://github.com/zsh-users/zsh-autosuggestions): the ghost-text approach
  our zsh plugin follows.
- [zsh-autocomplete](https://github.com/marlonrichert/zsh-autocomplete): real-time completion in zsh.
- [Fig / withfig/autocomplete](https://github.com/withfig/autocomplete) (now Amazon Q Developer CLI):
  our command knowledge starts from its MIT-licensed specs. See
  [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
- [atuin](https://github.com/atuinsh/atuin), [carapace](https://carapace.sh)
  and [fish](https://fishshell.com)'s built-in autosuggestions.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT)
at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

## Contributing

It's early. Read [`docs/design.md`](docs/design.md) first, and open an issue before a pull request
so we can check it fits the plan. Tests must never touch your real shell rc files or history.
