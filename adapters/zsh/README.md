# zsh adapter

Built in M0 (waiting for a check in a real terminal): `fuxx-autosuggestions.plugin.zsh` shows grey
ghost text from your history, from a `fuxx-completes serve` process. → or ⌃E accepts all of it,
and so does Tab while a suggestion shows (otherwise Tab completes as before); Esc f takes a word,
End and ⌥→ do the same where you haven't bound them; Esc hides the ghost text and clears a
completion list; vi mode works. A
command you run is reported to the core and suggested from the prompt after the next one ("one
prompt later"). Needs zsh 5.3 or later. Still planned: M2 (host channel), M6 (menu). Design:
`docs/design.md` §5.5–§5.9 and §6.

Try it by hand, in an isolated shell that never touches your real rc files or history. It works on
a copy of your history:

```
cargo build
try=$(mktemp -d) && (umask 077; cp "${HISTFILE:-$HOME/.zsh_history}" "$try/history")
env -i HOME="$try" TERM="$TERM" LANG="${LANG:-en_US.UTF-8}" PATH=/usr/bin:/bin zsh -d -i; rm -r "$try"
```

The copy lives in a private folder (mode 0700, the file 0600) and is deleted when the try-it shell
exits. Then, inside that new shell (`<repo>` is this checkout):

```
HISTFILE=$HOME/history; FUXX_COMPLETES_BIN=<repo>/target/debug/fuxx-completes
source <repo>/adapters/zsh/fuxx-autosuggestions.plugin.zsh
```

`exit` ends it and removes the copy; nothing in your normal shell changes. (`-d` skips the global
rc files and the temporary `HOME` has no `.zshrc`, so `-f` isn't needed.)

A thin zsh plugin, one file per library (for example `fuxx-autosuggestions.plugin.zsh` and
`fuxx-autocomplete.plugin.zsh`). It holds no ranking, parsing or command knowledge.

- **Ghost text:** ZLE's `POSTDISPLAY` holds the rest of the suggestion; a `region_highlight` entry
  dims it (default `fg=8`). Widgets that edit the buffer are wrapped, as zsh-autosuggestions does.
  We remove only our own `region_highlight` entries: tagged with `memo=` on zsh 5.9+, matched by
  their exact string on 5.8 and older.
- **Keys:** at the end of the line, `forward-char` and `end-of-line` accept the whole suggestion
  and `forward-word` accepts up to the next word. In stock zsh that's → and `^E`, and `Esc f`.
  End (`^[[F`, `^[[4~`) and ⌥→ (`^[[1;3C`) are bound by us **only if unbound**; a user's binding is
  never replaced.
- **Tab** (2026-11-13): with grey text showing at the end of the line, Tab accepts it; to
  complete instead, press Esc first (or move the cursor). While ghost text shows and the cursor is
  at the end, Tab accepts all of it, no completion runs, and a list left on screen is cleared. Otherwise the widget Tab is bound to runs as before (zsh's completion,
  fzf's `fzf-completion`, fzf-tab, ...), and the ghost text stays off until you edit again, so a
  second Tab goes on completing. Other completion keys (Shift-Tab, a ⌃Space bound to
  `complete-word`) keep it off the same way. Tab itself is never rebound: its widget is wrapped like
  the others.
- **Esc** (2026-11-13): clears a completion list and hides the ghost text until the next edit. In
  emacs mode a bare `^[` is bound **only if unbound**; zsh then waits `KEYTIMEOUT` (default 0.4 s)
  after Esc to tell it from Esc-prefixed keys, so it acts 0.4 s after the press, and Esc, a pause,
  then `f` is no longer `forward-word`. Keys sent as one sequence (⌥ as Meta, arrows, function
  keys, paste) are unaffected, and so is a key pressed right after Esc (↑, a paste, é); unbound
  sequences such as ⌃→ do nothing instead of typing their tail. In ⌃R, Esc ends the search on the line found. In vi mode Esc still enters command mode and
  clears both on the way (another key bound to `vi-cmd-mode`, such as `jk`, keeps the ghost text).
  `KEYTIMEOUT` and your options are never changed. Design §6.
- **vi mode** is supported: the vi accept and partial-accept widgets are wrapped as
  zsh-autosuggestions does (`vi-forward-char`, `vi-end-of-line`, `vi-add-eol`, `vi-forward-word`
  and friends).
- **Async:** requests go to the `fuxx-completes` process; answers arrive on a file descriptor
  watched with `zle -F`. Typing never waits.
- **Hooks:** `add-zsh-hook` and `add-zle-hook-widget`, never overwriting `precmd`, `preexec`,
  `zshaddhistory` or `zle-line-init`. Prints nothing at startup (safe with powerlevel10k's instant
  prompt).
- **History:** reports a command only if zsh kept it (`zshaddhistory`, checked at `precmd`;
  `HIST_IGNORE_SPACE`, `HIST_NO_STORE` and `HISTORY_IGNORE` respected).
- **Shell state (M5):** aliases with expansions, function names, builtins, `PATH` and named dirs
  go to the local core at a prompt where they changed. Never further.
- **Host channel (M2):** when the terminal sets `FUXX_COMPLETES_HOST=1`, the buffer and cursor go
  to the terminal over OSC 7701 instead of being drawn here, and `ESC [ 7701 ~` applies what the
  terminal accepted.
- **Menu (M6):** drawn below the line, not through zsh's completion system. Tab completes as above.
- **Stays off (M1, not built yet)** when zsh-autosuggestions (for ghost text) or zsh-autocomplete
  (`.autocomplete__*` or older `.autocomplete.*` functions, for the menu) is loaded, and entirely
  in Warp (`TERM_PROGRAM=WarpTerminal`), whose own editor replaces ZLE. Today none of that is
  detected. Never binds ⌃R, ⌃T or Tab (Tab's widget is wrapped, see above).
