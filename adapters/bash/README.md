# bash adapter

`fuxx-autosuggestions.bash` (M2): the best suggestion from your history on an explicit key, from
the same Rust core as the zsh plugin. Design: `docs/design.md` §3, §5.5, §5.8 and §6.

## Install

Put this at the **end** of `~/.bashrc`, with the `fuxx-completes` binary on `PATH` (or name it in
`FUXX_COMPLETES_BIN`):

```bash
source /path/to/fuxx-completes/adapters/bash/fuxx-autosuggestions.bash
```

With Homebrew (`brew install fitzfitz/fuxx/fuxx-completes`), the file is
`$(brew --prefix)/share/fuxx-completes/bash/fuxx-autosuggestions.bash` and the binary is on
`PATH`; the install prints the exact line. Other ways: the
[README's Install section](../../README.md#install).

macOS terminals (Terminal.app, iTerm) start bash as a login shell, which reads `~/.bash_profile`,
not `~/.bashrc`: make sure your `~/.bash_profile` sources `~/.bashrc`.

It starts its core at your first prompt and prints nothing, ever. `fuxx-completes doctor` (a
shell function it defines when you have none of that name) shows what it found.

## Keys

- **→** at the end of the line inserts the best suggestion; anywhere else it moves one character,
  as before.
- **End** at the end of the line inserts it too; anywhere else it moves to the end.
- On an **empty line right after a command failed**, the same keys insert the fix the core found
  (`lss` → `ls`). Nothing is ever run for you.

Each key waits **at most 50 ms** for the core; if the answer is late it does nothing. Keys that
only type are never bound and never wait. A key you (or another plugin) bound to something else
is left alone (`doctor` lists each key as `bound` or `kept`). Both emacs and vi insert mode;
vi command mode is left alone.

## What bash can't do (honestly)

- **No grey text as you type.** Plain readline has no `POSTDISPLAY` and no per-keystroke hook,
  and drawing behind readline's back breaks on redraw. ble.sh users get grey text (below).

## With ble.sh

[ble.sh](https://github.com/akinomyoga/ble.sh) (0.4 or later, on bash 4.0+) replaces readline
and draws its own grey auto-complete. When it is loaded, this adapter feeds it: our best
suggestion goes first in ble.sh's list, so ble.sh shows ours as its own grey text and its keys
(→, End, ⌃E, ⌃F) take it. Load ble.sh as its README says (`source ble.sh --noattach` at the top
of `~/.bashrc`, `ble-attach` at the end) and source this file before `ble-attach`. While ble.sh
is idle our source waits at most 50 ms for the core, checking every 5 ms, and stops the moment you
press a key. This uses an interface ble.sh's manual doesn't document (atuin uses the same one),
so a ble.sh update could break it. If atuin's ble.sh source is there too, ours steps aside.
The first command of a ble.sh session isn't reported (ble.sh loads the history after the first
prompt), and the first line you type may get no grey text while ble.sh loads its completion code.
- **bash 4.0–4.3:** every `bind -x` key moves to a new line and redraws the prompt (fixed in bash
  4.4), so there → is left to readline and only End inserts; each End press leaves the old line
  above. bash before 4.3 can't `bind -x` a key longer than two bytes, so End is a readline macro
  for a private `⌃X` sequence there (in vi insert mode `⌃X` then waits for a second key).
- **macOS's `/bin/bash` 3.2 is too old:** the adapter stays off and `doctor` says
  `bash-too-old`. Use a newer bash (Homebrew's, for example).
- **History:** a `PROMPT_COMMAND` entry (put first, so it sees the command's `$?`, which it hands
  on to the entries after it) reports a command when `history 1` shows a new entry. bash has
  applied `HISTCONTROL` and `HISTIGNORE` by then, so hidden lines are never reported. A repeat
  that `ignoredups` drops isn't counted again. An rc file that sets `PROMPT_COMMAND` again **after**
  sourcing us removes our entry: nothing is reported and the core never starts.
- The pipes to the core are inherited by programs you start (bash can't set close-on-exec), so a
  daemon started from this shell keeps the core alive until the daemon ends.
- A key pressed in the first moment after the shell starts may get nothing: the core is still
  starting.
- Stays off in Warp, whose own editor replaces readline.
- bash's own `complete` setup and Tab are left alone.
