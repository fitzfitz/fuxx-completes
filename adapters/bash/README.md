# bash adapter

Not built yet. Planned for M2 (suggestions) and M7 (menu). Design: `docs/design.md` §6.

What bash allows, honestly:

- **Plain readline has no way to show ghost text** as you type: no `POSTDISPLAY` equivalent and no
  per-keystroke hook. We won't draw grey text behind readline's back.
- **What works:** `bind -x` runs a function on a key and can read and set `READLINE_LINE` and
  `READLINE_POINT`. So a key can insert the best suggestion, and a key can show a menu.
- **Needs bash 4 or later.** macOS ships bash 3.2, which lacks `READLINE_LINE`; use Homebrew's bash.
- **ble.sh** replaces readline and has its own grey auto-complete. Whether we can feed it our
  suggestions is an open question for M2; if not, we stay off when ble.sh is loaded.
- bash's own `complete` setup and Tab are left alone.
