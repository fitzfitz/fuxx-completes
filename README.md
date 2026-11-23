# fuxx-completes

Ghost-text suggestions and an as-you-type completion menu for zsh, bash and fish, from one Rust
engine that doesn't rely on any shell's completion system.

> **Status: fuxx-autosuggestions 0.1.0, the first release, for macOS 11+ (Apple Silicon and
> Intel).** Ghost text in zsh, suggestions on a key (or ble.sh's grey text) in bash, and your fish
> commands counted in your other shells. Still pre-1.0: settings and the crates' APIs may change
> in a minor version. **fuxx-autocomplete (the menu) is not built yet.** Linux comes later. The
> design is in [`docs/design.md`](docs/design.md), the progress in
> [`docs/roadmap.md`](docs/roadmap.md).

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

`fuxx-completes why git push` explains why a line is (or isn't) suggested where you are: its runs
here, in this repo and anywhere, the project file it came from, and whether it is hidden or
guarded. `fuxx-completes doctor --report` prints the doctor output in a block ready to paste into
an issue (check it first: it shows your data folder's path). Neither sends anything anywhere.

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

## Install

Two parts: the `fuxx-completes` binary, and one line in your shell's rc file that loads the
adapter for that shell. Nothing is turned on for you, and nothing edits your rc files.

### Homebrew (macOS)

```sh
brew install fitzfitz/fuxx/fuxx-completes
```

This installs the binary and the zsh, bash and fish adapters (under
`share/fuxx-completes/` in Homebrew's prefix), and prints the exact line for each shell. With
Homebrew in `/opt/homebrew` (Apple Silicon; `/usr/local` on Intel):

```sh
# zsh, in ~/.zshrc
source /opt/homebrew/share/fuxx-completes/zsh/fuxx-autosuggestions.plugin.zsh
# bash 4.0+, at the end of ~/.bashrc
source /opt/homebrew/share/fuxx-completes/bash/fuxx-autosuggestions.bash
# fish (reports your commands; fish keeps its own suggestions)
ln -s /opt/homebrew/share/fuxx-completes/fish/fuxx-autosuggestions.fish ~/.config/fish/conf.d/
```

Open a new shell; `fuxx-completes doctor` shows what is running. The paths stay the same across
`brew upgrade`. Install by the full name `fitzfitz/fuxx/fuxx-completes` as above: Homebrew loads
formulae from taps that aren't its own only once trusted, and installing by full name trusts this
one.

### zsh plugin managers

A plugin manager loads the zsh adapter from this repo; the **binary still comes from Homebrew**
(above) or from `cargo install fuxx-completes` (below), or name it in `FUXX_COMPLETES_BIN`.

- **Plain `source`**, from a clone:
  ```sh
  git clone https://github.com/fitzfitz/fuxx-completes ~/fuxx-completes
  # in ~/.zshrc:
  source ~/fuxx-completes/adapters/zsh/fuxx-autosuggestions.plugin.zsh
  ```
- **oh-my-zsh**: link the adapter's folder in as a custom plugin, then add it to `plugins`:
  ```sh
  git clone https://github.com/fitzfitz/fuxx-completes ~/fuxx-completes
  ln -s ~/fuxx-completes/adapters/zsh "${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}/plugins/fuxx-autosuggestions"
  # in ~/.zshrc, before `source $ZSH/oh-my-zsh.sh`:
  plugins=(git fuxx-autosuggestions)
  ```
- **zinit**, in `~/.zshrc`:
  ```sh
  zinit ice pick"adapters/zsh/fuxx-autosuggestions.plugin.zsh"
  zinit light fitzfitz/fuxx-completes
  ```
- **antidote**, in `~/.zsh_plugins.txt`:
  ```
  fitzfitz/fuxx-completes path:adapters/zsh
  ```
- **zap**: clone, then point `plug` at the file:
  ```sh
  git clone https://github.com/fitzfitz/fuxx-completes ~/fuxx-completes
  # in ~/.zshrc, after zap's own line:
  plug "$HOME/fuxx-completes/adapters/zsh/fuxx-autosuggestions.plugin.zsh"
  ```

Two copies in one shell (say Homebrew's line and fuxx's plugin) are fine: the first one loaded
runs, the other stays off, and `doctor` names both.

### bash

bash 4.0 or later (macOS's `/bin/bash` is 3.2, too old: use Homebrew's `bash`). Put the line at
the **end** of `~/.bashrc`: Homebrew's path above, or
`source ~/fuxx-completes/adapters/bash/fuxx-autosuggestions.bash` from a clone. macOS terminals
start bash as a login shell, which reads `~/.bash_profile`, not `~/.bashrc`: make sure your
`~/.bash_profile` sources `~/.bashrc`. With ble.sh,
source it before `ble-attach`. What bash can and can't do:
[`adapters/bash/README.md`](adapters/bash/README.md).

### fish

fish keeps its own autosuggestions; ours only reports the commands you run, so they count in your
other shells. Link the file into `~/.config/fish/conf.d/` (Homebrew's path above, or
`~/fuxx-completes/adapters/fish/fuxx-autosuggestions.fish` from a clone). Details:
[`adapters/fish/README.md`](adapters/fish/README.md).

### In fuxx

[fuxx](https://github.com/fitzfitz/fuxx-cli) installs it as a plugin: **Settings → Plugins**, paste
`github.com/fitzfitz/fuxx-completes`. fuxx downloads the release zip for your Mac's chip, puts the
binary on `PATH` in its terminals and loads the zsh adapter; the list's settings are on the
plugin's card. Nothing else to do.

### From crates.io

```sh
cargo install fuxx-completes
```

builds the binary from source (Rust needed); add an adapter from a clone or a plugin manager as
above. Apps that embed the engine use the crates `fuxx-completes-core` and `fuxx-autosuggestions`.

**Not** by downloading a release zip in a browser: macOS quarantines the download and blocks the
binary, which isn't notarised. The zips on the releases page are for fuxx, which fetches them
itself.

### Upgrading and uninstalling

- Upgrade: `brew upgrade fuxx-completes`; a plugin manager's own update (`zinit update`,
  `antidote update`, `git pull` in your clone); fuxx offers the update in Settings → Plugins.
- Uninstall: run `fuxx-completes forget` first to wipe the context file and its secret, remove
  the line from your rc file (or the fish link), then `brew uninstall fuxx-completes` (or
  `cargo uninstall fuxx-completes`). Your history files are never touched either way.

## Shell support

| | Ghost text (fuxx-autosuggestions, 0.1.0) | Menu (fuxx-autocomplete, planned) |
| --- | --- | --- |
| **zsh** 5.3+ | As you type, plus a list of up to 5 when you pause | As you type, below the line; Tab stays zsh's |
| **bash** 4.0+ | Plain readline: key-driven only, → or End at the end of the line inserts the best suggestion (readline can't show ghost text). With ble.sh (0.4+), ble.sh's own grey text shows ours | Key-driven |
| **fish** 3.0+ | No: fish has its own, and ours stays off; your fish commands still count in your other shells | On Tab, in fish's own pager, only for commands fish has no completions for |

macOS 11 or later on Apple Silicon or Intel (the Intel build is made on Apple Silicon and tested
under Rosetta). Linux later, no Windows. fish is tested on 4.x; 3.x is untested so far.

Works in most terminals: Ghostty, iTerm, Terminal.app and others. **Not in Warp:** Warp's own input
editor replaces the shell's, so shell plugins like this can't run there, and both libraries stay
off. Not used inside AI agents such as Claude Code, which have their own suggestions. If you
already use zsh-autosuggestions or zsh-autocomplete, the matching fuxx library stays off. It never
takes ⌃R, ⌃T or Tab from you.

In bash, the accept keys may wait up to 50 ms for an answer (and do nothing if it's late); typing
itself never waits, in any shell.

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

### For terminal authors: the host channel

A terminal that wants to draw the suggestions itself sets `FUXX_COMPLETES_HOST=1` in the shell's
environment. The zsh and bash plugins then start no core and draw nothing; instead they write
`ESC ] 7701 ; fuxx-completes ; 1 ; <message> ; <fields…> BEL` to the terminal: `hello` (shell,
version, history format and file), `buffer` (an id, the cursor, the folder and the line: in zsh
as you type, in bash at → or End), `clear`, and `ran`/`prev` for the commands the shell saved.
Text fields are base64, numbers plain digits. To put a line in, the terminal types
`ESC [ 7701 ~`, base64 of `<id> 0x1F <line> 0x1F <cursor>`, then `BEL`; the shell applies it only
for the id of the line it last sent. Inside tmux or screen the channel stays off. The Rust crate
`fuxx-completes-core` has the codec (`host::scan`, `host::encode_accept`); `docs/design.md` §5.5
has the details.

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
core/                      fuxx-completes-core: the shared engine (project mappings in core/projects/)
cli/                       the fuxx-completes binary
fuxx-autosuggestions/      the ghost-text library
fuxx-autocomplete/         the menu library
specs/                     command knowledge (our own format)
adapters/zsh|bash|fish/    thin shell plugins
docs/design.md             the design (source of truth)
docs/roadmap.md            milestones and progress
scripts/                   test-shell builds and matrices; package.sh and formula.sh for releases
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
