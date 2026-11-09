# zsh adapter

Not built yet. Planned for M0 (ghost text) and M6 (menu). Design: `docs/design.md` §6.

A thin zsh plugin, one file per library (for example `fuxx-autosuggestions.plugin.zsh` and
`fuxx-autocomplete.plugin.zsh`). It holds no ranking, parsing or command knowledge.

- **Ghost text:** ZLE's `POSTDISPLAY` holds the rest of the suggestion; a `region_highlight` entry
  dims it (default `fg=8`). Widgets that edit the buffer are wrapped, as zsh-autosuggestions does.
- **Keys:** → (`forward-char`) or End (`end-of-line`) at the end of the line accepts the whole
  suggestion; ⌥→ (`forward-word`) accepts up to the next word.
- **Async:** requests go to the `fuxx-completes` process; answers arrive on a file descriptor
  watched with `zle -F`. Typing never waits.
- **Hooks:** `add-zsh-hook` and `add-zle-hook-widget`, never overwriting `precmd`, `preexec` or
  `zle-line-init`. Prints nothing at startup (safe with powerlevel10k's instant prompt).
- **Menu (M6):** drawn below the line, not through zsh's completion system. Tab stays zsh's.
- **Stays off** when zsh-autosuggestions (for ghost text) or zsh-autocomplete (for the menu) is
  loaded. Never binds ⌃R, ⌃T or Tab.
