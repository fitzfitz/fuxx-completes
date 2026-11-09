# zsh adapter

Not built yet. Planned for M0 (ghost text), M2 (host channel) and M6 (menu). Design:
`docs/design.md` §5.5–§5.9 and §6. Needs zsh 5.3 or later.

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
- **Menu (M6):** drawn below the line, not through zsh's completion system. Tab stays zsh's.
- **Stays off** when zsh-autosuggestions (for ghost text) or zsh-autocomplete (`.autocomplete__*`
  or older `.autocomplete.*` functions, for the menu) is loaded, and entirely in Warp
  (`TERM_PROGRAM=WarpTerminal`), whose own editor replaces ZLE. Never binds ⌃R, ⌃T or Tab.
